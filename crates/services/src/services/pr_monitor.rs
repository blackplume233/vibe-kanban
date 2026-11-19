use std::{sync::Arc, time::Duration};

use api_types::{PullRequestStatus, UpdatePullRequestApiRequest, UpsertPullRequestRequest};
use chrono::Utc;
use db::{
    DBService,
    models::{
        merge::MergeStatus,
        pull_request::PullRequest,
        workspace::{Workspace, WorkspaceError},
    },
};
use git_host::{
    GitHostError, GitHostProvider, GitHostService, PullRequestDetail, UnifiedPrComment,
    github::{GitHubProvider, PR_BATCH_SIZE},
};
use serde_json::json;
use sqlx::error::Error as SqlxError;
use thiserror::Error;
use tokio::{
    sync::Notify,
    time::{Instant, interval, sleep_until},
};
use tracing::{debug, error, info, warn};

use crate::services::{
    analytics::AnalyticsContext,
    container::ContainerService,
    pr_follow_up::{self, PrFollowUpError},
    remote_client::{RemoteClient, RemoteClientError},
    remote_sync,
};

const POLL_INTERVAL: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(15 * 60);

fn poll_delay(failures: u32, retry_after: Duration, jitter: u16) -> Duration {
    let delay = POLL_INTERVAL
        .saturating_mul(1 << failures.min(8))
        .min(MAX_BACKOFF);
    let spread_ms = delay.as_millis() as u64 / 5;
    delay.max(retry_after).saturating_add(Duration::from_millis(
        spread_ms * u64::from(jitter) / u64::from(u16::MAX),
    ))
}

#[derive(Default)]
struct PollOutcome {
    failed: bool,
    retry_after: Duration,
}

impl PollOutcome {
    fn record_error(&mut self, error: &GitHostError) {
        self.failed = true;
        if let GitHostError::RateLimited { retry_after } = error {
            self.retry_after = self.retry_after.max(*retry_after);
        }
    }
}

#[derive(Debug, Error)]
enum PrMonitorError {
    #[error(transparent)]
    GitHostError(#[from] GitHostError),
    #[error(transparent)]
    WorkspaceError(#[from] WorkspaceError),
    #[error(transparent)]
    Sqlx(#[from] SqlxError),
    #[error(transparent)]
    FollowUp(#[from] PrFollowUpError),
}

/// Service to monitor PRs and update task status when they are merged
pub struct PrMonitorService<C: ContainerService> {
    db: DBService,
    review_enabled: bool,
    analytics: Option<AnalyticsContext>,
    container: C,
    remote_client: Option<RemoteClient>,
    sync_notify: Arc<Notify>,
}

impl<C: ContainerService + Send + Sync + 'static> PrMonitorService<C> {
    pub async fn spawn(
        db: DBService,
        analytics: Option<AnalyticsContext>,
        container: C,
        remote_client: Option<RemoteClient>,
        sync_notify: Arc<Notify>,
    ) -> tokio::task::JoinHandle<()> {
        let service = Self {
            db,
            review_enabled: std::env::var("VK_GITHUB_REVIEW_AUTO_FOLLOW_UP")
                .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
            analytics,
            container,
            remote_client,
            sync_notify,
        };
        tokio::spawn(async move {
            service.start().await;
        })
    }

    async fn start(&self) {
        info!("Starting PR monitoring service");
        let mut next_poll = Instant::now();
        let mut consecutive_failures = 0_u32;
        let mut remote_sync_tick = interval(POLL_INTERVAL);
        // The first tick would duplicate the initial poll's sync sweep.
        remote_sync_tick.tick().await;
        loop {
            tokio::select! {
                _ = sleep_until(next_poll) => {
                    let outcome = match self.check_all_open_prs().await {
                        Ok(outcome) => outcome,
                        Err(e) => {
                            error!("Error checking open PRs: {e}");
                            PollOutcome { failed: true, ..PollOutcome::default() }
                        }
                    };
                    consecutive_failures = if outcome.failed { consecutive_failures.saturating_add(1) } else { 0 };
                    let jitter = uuid::Uuid::new_v4().as_u128() as u16;
                    next_poll = Instant::now() + poll_delay(consecutive_failures, outcome.retry_after, jitter);
                }
                _ = self.sync_notify.notified() => debug!("PR sync triggered externally"),
                _ = remote_sync_tick.tick(), if self.remote_client.is_some() => {}
            }
            self.sync_pending_to_remote().await;
        }
    }

    async fn check_all_open_prs(&self) -> Result<PollOutcome, PrMonitorError> {
        let mut outcome = PollOutcome::default();
        let open_prs = PullRequest::get_open(&self.db.pool).await?;
        let (github, other): (Vec<_>, Vec<_>) = open_prs
            .iter()
            .partition(|pr| GitHubProvider::supports_batch(&pr.pr_url));
        let provider = GitHubProvider::new()?;
        for prs in github.chunks(PR_BATCH_SIZE) {
            let urls = prs.iter().map(|pr| pr.pr_url.as_str()).collect::<Vec<_>>();
            let results = match provider.poll_batch(&urls, self.review_enabled).await {
                Ok(results) => results,
                Err(error) => {
                    warn!("GitHub batched PR polling failed: {error}");
                    outcome.record_error(&error);
                    break;
                }
            };
            for (pr, result) in prs.iter().zip(results) {
                match result {
                    Ok(snapshot) => {
                        self.apply_snapshot(pr, snapshot.detail, snapshot.review_comments)
                            .await
                    }
                    Err(error) => {
                        warn!("Skipping GitHub PR #{}: {error}", pr.pr_number);
                        if error.should_retry() || matches!(error, GitHostError::RateLimited { .. })
                        {
                            outcome.record_error(&error);
                        }
                    }
                }
            }
            if !outcome.retry_after.is_zero() {
                break;
            }
        }
        for pr in other {
            let result = async {
                let host = GitHostService::from_url(&pr.pr_url)?;
                let status = host.get_pr_status(&pr.pr_url).await?;
                let comments = if self.review_enabled
                    && matches!(status.status, MergeStatus::Open)
                    && let GitHostService::GitHub(host) = host
                {
                    host.get_inline_review_comments(&pr.pr_url).await?
                } else {
                    Vec::new()
                };
                Ok::<_, GitHostError>((status, comments))
            }
            .await;
            match result {
                Ok((status, comments)) => self.apply_snapshot(pr, status, comments).await,
                Err(error) => warn!("Skipping PR #{}: {error}", pr.pr_number),
            }
        }
        Ok(outcome)
    }

    async fn apply_snapshot(
        &self,
        pr: &PullRequest,
        detail: PullRequestDetail,
        comments: Vec<UnifiedPrComment>,
    ) {
        let result = if matches!(detail.status, MergeStatus::Open) {
            self.follow_up_on_review(pr, comments).await
        } else {
            self.update_terminal_status(pr, detail).await
        };
        if let Err(error) = result {
            error!("Failed to apply PR #{} update: {error}", pr.pr_number);
        }
    }

    async fn follow_up_on_review(
        &self,
        pr: &PullRequest,
        comments: Vec<UnifiedPrComment>,
    ) -> Result<(), PrMonitorError> {
        let Some(workspace_id) = pr.workspace_id.filter(|_| self.review_enabled) else {
            return Ok(());
        };
        if comments.is_empty() {
            return Ok(());
        }
        let Some(workspace) = Workspace::find_by_id(&self.db.pool, workspace_id).await? else {
            return Ok(());
        };
        let handled = PullRequest::handled_review_comment_ids(&self.db.pool, &pr.id).await?;
        let Some((prompt, ids)) =
            pr_follow_up::review_prompt(&pr.pr_url, pr.created_at, comments, handled)
        else {
            return Ok(());
        };
        match pr_follow_up::save_review_draft(&self.db.pool, &workspace, pr, prompt, &ids).await {
            Ok(()) | Err(PrFollowUpError::NoAgent) => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    async fn update_terminal_status(
        &self,
        pr: &PullRequest,
        status: PullRequestDetail,
    ) -> Result<(), PrMonitorError> {
        debug!(
            "PR #{} status: {:?} (was open)",
            pr.pr_number, status.status
        );

        if matches!(&status.status, MergeStatus::Unknown) {
            return Ok(());
        }

        let merged_at = if matches!(&status.status, MergeStatus::Merged) {
            Some(status.merged_at.unwrap_or_else(Utc::now))
        } else {
            None
        };

        PullRequest::update_status(
            &self.db.pool,
            &pr.pr_url,
            &status.status,
            merged_at,
            status.merge_commit_sha,
        )
        .await?;

        // If this is a workspace PR and it was merged, try to archive
        if matches!(&status.status, MergeStatus::Merged)
            && let Some(workspace_id) = pr.workspace_id
        {
            self.try_archive_workspace(workspace_id, pr.pr_number)
                .await?;
        }

        info!("PR #{} status changed to {:?}", pr.pr_number, status.status);

        Ok(())
    }

    /// Archive workspace if all its PRs are merged/closed
    async fn try_archive_workspace(
        &self,
        workspace_id: uuid::Uuid,
        pr_number: i64,
    ) -> Result<(), PrMonitorError> {
        let Some(workspace) = Workspace::find_by_id(&self.db.pool, workspace_id).await? else {
            return Ok(());
        };

        let open_pr_count =
            PullRequest::count_open_for_workspace(&self.db.pool, workspace_id).await?;

        if open_pr_count == 0 {
            info!(
                "PR #{} was merged, archiving workspace {}",
                pr_number, workspace.id
            );
            if !workspace.pinned
                && let Err(e) = self.container.archive_workspace(workspace.id).await
            {
                error!("Failed to archive workspace {}: {}", workspace.id, e);
            }

            if let Some(analytics) = &self.analytics {
                analytics.analytics_service.track_event(
                    &analytics.user_id,
                    "pr_merged",
                    Some(json!({
                        "workspace_id": workspace.id.to_string(),
                    })),
                );
            }
        } else {
            info!(
                "PR #{} was merged, leaving workspace {} active with {} open PR(s)",
                pr_number, workspace.id, open_pr_count
            );
        }

        Ok(())
    }

    /// Sync pending PR status changes to remote server.
    async fn sync_pending_to_remote(&self) {
        let Some(client) = &self.remote_client else {
            return;
        };

        let pending = match PullRequest::get_pending_sync(&self.db.pool).await {
            Ok(prs) => prs,
            Err(e) => {
                error!("Failed to query pending sync PRs: {}", e);
                return;
            }
        };

        if pending.is_empty() {
            return;
        }

        debug!("Syncing {} pending PRs to remote", pending.len());

        for pr in &pending {
            let pr_api_status = match &pr.pr_status {
                MergeStatus::Open => PullRequestStatus::Open,
                MergeStatus::Merged => PullRequestStatus::Merged,
                MergeStatus::Closed => PullRequestStatus::Closed,
                MergeStatus::Unknown => continue,
            };

            let request = UpdatePullRequestApiRequest {
                url: pr.pr_url.clone(),
                status: Some(pr_api_status),
                merged_at: pr.merged_at.map(Some),
                merge_commit_sha: pr.merge_commit_sha.clone().map(Some),
            };

            match client.update_pull_request(request).await {
                Ok(_) => {
                    if let Err(e) = PullRequest::mark_synced(&self.db.pool, &pr.id).await {
                        error!("Failed to mark PR #{} as synced: {}", pr.pr_number, e);
                    }
                }
                Err(RemoteClientError::Http { status: 404, .. }) => {
                    if let Some(workspace_id) = pr.workspace_id {
                        let request = UpsertPullRequestRequest {
                            url: pr.pr_url.clone(),
                            number: pr.pr_number as i32,
                            status: pr_api_status,
                            merged_at: pr.merged_at,
                            merge_commit_sha: pr.merge_commit_sha.clone(),
                            target_branch_name: pr.target_branch_name.clone(),
                            local_workspace_id: workspace_id,
                        };
                        remote_sync::sync_pr_to_remote(client, request).await;
                        if let Err(e) = PullRequest::mark_synced(&self.db.pool, &pr.id).await {
                            error!("Failed to mark PR #{} as synced: {}", pr.pr_number, e);
                        }
                    } else {
                        warn!(
                            "PR #{} not found on remote and has no workspace, removing local record",
                            pr.pr_number
                        );
                        if let Err(e) = PullRequest::delete(&self.db.pool, &pr.id).await {
                            error!("Failed to delete orphaned local PR: {}", e);
                        }
                    }
                }
                Err(RemoteClientError::Auth) => {
                    debug!("PR sync sweep stopped: not authenticated");
                    return;
                }
                Err(e) => {
                    error!(
                        "Failed to sync PR #{} status to remote: {}",
                        pr.pr_number, e
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polling_uses_capped_exponential_backoff_jitter_and_github_reset() {
        let delay = |failures, jitter| poll_delay(failures, Duration::ZERO, jitter).as_secs();
        assert_eq!(delay(0, 0), 60);
        assert_eq!(delay(1, 0), 120);
        assert_eq!(delay(2, u16::MAX), 288);
        assert_eq!(delay(u32::MAX, 0), 900);
        assert_eq!(poll_delay(2, Duration::from_secs(1800), 0).as_secs(), 1800);
    }
}
