use async_trait::async_trait;
use db::models::{
    issue::{Issue, require_project},
    issue_board::IssueStatus,
    issue_sync::{GitHubIssueSyncUpdate, IssueSyncFields, IssueSyncStoreError, LinkedGitHubIssue},
};
use git_host::{
    GitHostError,
    github::{GitHubIssue, GitHubIssueClient, GitHubIssuePatch, GitHubRepository, IssueState},
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::Mutex;
use ts_rs::TS;
use uuid::Uuid;

// A single local process serializes sync requests, including across projects sharing an issue.
static SYNC_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Deserialize, TS)]
pub struct SyncGitHubIssueRequest {
    pub repository: String,
    pub number: u32,
    pub open_status_id: Uuid,
    pub closed_status_id: Uuid,
    #[serde(default)]
    #[ts(optional)]
    pub write_back: Option<bool>,
}

#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum IssueSyncOutcome {
    Imported,
    Synced,
    Unchanged,
    PendingWrite,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum IssueSyncField {
    Title,
    Description,
    State,
    RemoteIdentity,
}

#[derive(Debug, Serialize, TS)]
pub struct SyncGitHubIssueResponse {
    pub issue: Issue,
    pub outcome: IssueSyncOutcome,
    pub fields: Vec<IssueSyncField>,
}

#[derive(Debug, thiserror::Error)]
pub enum IssueSyncError {
    #[error("{0}")]
    Invalid(&'static str),
    #[error(transparent)]
    GitHub(#[from] GitHostError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Store(#[from] IssueSyncStoreError),
}

#[async_trait]
trait IssueRemote: Sync {
    async fn get(&self, repo: &GitHubRepository, number: u32) -> Result<GitHubIssue, GitHostError>;
    async fn update(
        &self,
        repo: &GitHubRepository,
        number: u32,
        patch: &GitHubIssuePatch<'_>,
    ) -> Result<GitHubIssue, GitHostError>;
}

#[async_trait]
impl IssueRemote for GitHubIssueClient {
    async fn get(&self, repo: &GitHubRepository, number: u32) -> Result<GitHubIssue, GitHostError> {
        self.get(repo, number).await
    }

    async fn update(
        &self,
        repo: &GitHubRepository,
        number: u32,
        patch: &GitHubIssuePatch<'_>,
    ) -> Result<GitHubIssue, GitHostError> {
        self.update(repo, number, patch).await
    }
}

pub async fn sync_github_issue(
    pool: &SqlitePool,
    project_id: Uuid,
    request: &SyncGitHubIssueRequest,
) -> Result<SyncGitHubIssueResponse, IssueSyncError> {
    let remote = GitHubIssueClient::new()?;
    sync(pool, project_id, request, &remote).await
}

fn fields(issue: &GitHubIssue) -> IssueSyncFields {
    IssueSyncFields {
        title: issue.title.trim().to_owned(),
        description: issue.body.as_ref().filter(|body| !body.is_empty()).cloned(),
        closed: issue.state == IssueState::Closed,
    }
}

fn differences(a: &IssueSyncFields, b: &IssueSyncFields) -> Vec<IssueSyncField> {
    let mut fields = Vec::new();
    if a.title != b.title {
        fields.push(IssueSyncField::Title);
    }
    if a.description != b.description {
        fields.push(IssueSyncField::Description);
    }
    if a.closed != b.closed {
        fields.push(IssueSyncField::State);
    }
    fields
}

fn conflicts(
    local: &IssueSyncFields,
    remote: &IssueSyncFields,
    baseline: Option<&IssueSyncFields>,
) -> Vec<IssueSyncField> {
    fn conflict<T: PartialEq>(local: &T, remote: &T, baseline: Option<&T>) -> bool {
        local != remote && baseline.is_none_or(|base| base != local && base != remote)
    }
    let mut result = Vec::new();
    if conflict(
        &local.title,
        &remote.title,
        baseline.map(|base| &base.title),
    ) {
        result.push(IssueSyncField::Title);
    }
    if conflict(
        &local.description,
        &remote.description,
        baseline.map(|base| &base.description),
    ) {
        result.push(IssueSyncField::Description);
    }
    if conflict(
        &local.closed,
        &remote.closed,
        baseline.map(|base| &base.closed),
    ) {
        result.push(IssueSyncField::State);
    }
    result
}

fn merge(
    local: &IssueSyncFields,
    remote: &IssueSyncFields,
    baseline: &IssueSyncFields,
) -> IssueSyncFields {
    IssueSyncFields {
        title: if local.title == baseline.title {
            &remote.title
        } else {
            &local.title
        }
        .clone(),
        description: if local.description == baseline.description {
            &remote.description
        } else {
            &local.description
        }
        .clone(),
        closed: if local.closed == baseline.closed {
            remote.closed
        } else {
            local.closed
        },
    }
}

fn conflict_response(issue: Issue, fields: Vec<IssueSyncField>) -> SyncGitHubIssueResponse {
    SyncGitHubIssueResponse {
        issue,
        outcome: IssueSyncOutcome::Conflict,
        fields,
    }
}

async fn sync(
    pool: &SqlitePool,
    project_id: Uuid,
    request: &SyncGitHubIssueRequest,
    remote: &impl IssueRemote,
) -> Result<SyncGitHubIssueResponse, IssueSyncError> {
    let repository: GitHubRepository = request
        .repository
        .parse()
        .map_err(|_| IssueSyncError::Invalid("Expected a GitHub repository in owner/name form"))?;
    if request.number == 0 {
        return Err(IssueSyncError::Invalid(
            "The GitHub issue number must be positive",
        ));
    }
    require_project(pool, project_id).await?;
    let open = IssueStatus::get(pool, project_id, request.open_status_id).await?;
    let closed = IssueStatus::get(pool, project_id, request.closed_status_id).await?;
    if IssueSyncFields::is_closed(open.category) || !IssueSyncFields::is_closed(closed.category) {
        return Err(IssueSyncError::Invalid(
            "Map open issues to an active status and closed issues to a completed or canceled status",
        ));
    }
    let status_id = |closed: bool| {
        if closed {
            request.closed_status_id
        } else {
            request.open_status_id
        }
    };
    let write_back = request.write_back.unwrap_or(false);
    let external_id = format!("{repository}#{}", request.number);
    let _sync = SYNC_LOCK.lock().await;
    let linked = LinkedGitHubIssue::load(pool, project_id, &external_id).await?;
    let mut github = remote.get(&repository, request.number).await?;
    let mut remote_fields = fields(&github);
    let Some(linked) = linked else {
        let issue = LinkedGitHubIssue::save(
            pool,
            project_id,
            None,
            GitHubIssueSyncUpdate {
                external_id: &external_id,
                external_url: &github.html_url,
                remote_issue_id: github.id,
                remote: &remote_fields,
                local: &remote_fields,
                status_id: status_id(remote_fields.closed),
            },
        )
        .await?;
        return Ok(SyncGitHubIssueResponse {
            issue,
            outcome: IssueSyncOutcome::Imported,
            fields: Vec::new(),
        });
    };
    if linked
        .baseline
        .as_ref()
        .is_some_and(|base| base.remote_issue_id != github.id)
    {
        return Ok(conflict_response(
            linked.issue,
            vec![IssueSyncField::RemoteIdentity],
        ));
    }
    let baseline = linked.baseline.as_ref().map(|base| &base.fields);
    let overlap = conflicts(&linked.fields, &remote_fields, baseline);
    if !overlap.is_empty() {
        return Ok(conflict_response(linked.issue, overlap));
    }
    let desired = merge(
        &linked.fields,
        &remote_fields,
        baseline.unwrap_or(&remote_fields),
    );
    let outgoing = differences(&desired, &remote_fields);
    let incoming = differences(&desired, &linked.fields);
    if write_back && !outgoing.is_empty() {
        // GitHub REST does not support atomic conditional PATCH on this resource.
        // Revalidate the write snapshot immediately before sending the field-level update.
        let latest_local = LinkedGitHubIssue::load(pool, project_id, &external_id).await?;
        if !latest_local
            .as_ref()
            .is_some_and(|current| linked.matches(current))
        {
            return Err(IssueSyncStoreError::ConcurrentChange.into());
        }
        let latest = remote.get(&repository, request.number).await?;
        if latest.id != github.id {
            return Ok(conflict_response(
                linked.issue,
                vec![IssueSyncField::RemoteIdentity],
            ));
        }
        let concurrent = differences(&fields(&latest), &remote_fields);
        if !concurrent.is_empty() {
            return Ok(conflict_response(linked.issue, concurrent));
        }
        let patch = GitHubIssuePatch {
            title: (desired.title != remote_fields.title).then_some(desired.title.as_str()),
            body: (desired.description != remote_fields.description)
                .then_some(desired.description.as_deref().unwrap_or("")),
            state: (desired.closed != remote_fields.closed).then_some(if desired.closed {
                IssueState::Closed
            } else {
                IssueState::Open
            }),
        };
        let updated = remote.update(&repository, request.number, &patch).await?;
        if updated.id != github.id {
            return Ok(conflict_response(
                linked.issue,
                vec![IssueSyncField::RemoteIdentity],
            ));
        }
        let updated_fields = fields(&updated);
        let divergent = differences(&desired, &updated_fields);
        if !divergent.is_empty() {
            return Ok(conflict_response(linked.issue, divergent));
        }
        github = updated;
        remote_fields = updated_fields;
    }
    let desired_status = if desired.closed == linked.fields.closed {
        linked.issue.status_id
    } else {
        status_id(desired.closed)
    };
    let issue = LinkedGitHubIssue::save(
        pool,
        project_id,
        Some(&linked),
        GitHubIssueSyncUpdate {
            external_id: &external_id,
            external_url: &github.html_url,
            remote_issue_id: github.id,
            remote: &remote_fields,
            local: &desired,
            status_id: desired_status,
        },
    )
    .await?;
    let (outcome, fields) = if !write_back && !outgoing.is_empty() {
        (IssueSyncOutcome::PendingWrite, outgoing)
    } else if incoming.is_empty() && outgoing.is_empty() {
        (IssueSyncOutcome::Unchanged, Vec::new())
    } else {
        let mut changed = incoming;
        for field in outgoing {
            if !changed.contains(&field) {
                changed.push(field);
            }
        }
        (IssueSyncOutcome::Synced, changed)
    };
    Ok(SyncGitHubIssueResponse {
        issue,
        outcome,
        fields,
    })
}

#[cfg(test)]
mod tests {
    use db::models::{
        issue::UpdateIssue,
        issue_board::{IssueStatusCategory, SaveIssueStatus},
        project::Project,
    };
    use serde_json::json;

    use super::*;

    struct FixtureRemote(std::sync::Mutex<(IssueSyncFields, usize)>);

    impl FixtureRemote {
        fn response(content: &IssueSyncFields) -> GitHubIssue {
            serde_json::from_value(json!({
                "id": 8123456789_i64,
                "number": 42,
                "html_url": "https://github.com/acme/repo/issues/42",
                "title": content.title,
                "body": content.description,
                "state": if content.closed { "closed" } else { "open" }
            }))
            .unwrap()
        }
    }

    #[async_trait]
    impl IssueRemote for FixtureRemote {
        async fn get(
            &self,
            _repo: &GitHubRepository,
            _number: u32,
        ) -> Result<GitHubIssue, GitHostError> {
            Ok(Self::response(&self.0.lock().unwrap().0))
        }

        async fn update(
            &self,
            _repo: &GitHubRepository,
            _number: u32,
            patch: &GitHubIssuePatch<'_>,
        ) -> Result<GitHubIssue, GitHostError> {
            let mut state = self.0.lock().unwrap();
            state.1 += 1;
            assert!(!patch.is_empty());
            if let Some(title) = patch.title {
                state.0.title = title.into();
            }
            if let Some(body) = patch.body {
                state.0.description = (!body.is_empty()).then(|| body.into());
            }
            if let Some(status) = patch.state {
                state.0.closed = status == IssueState::Closed;
            }
            Ok(Self::response(&state.0))
        }
    }

    #[tokio::test]
    async fn imports_syncs_both_directions_idempotently_and_preserves_conflicts() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../db/migrations").run(&pool).await.unwrap();
        let project = Project::create(&pool, "Test project").await.unwrap();
        let status = |name: &str, category| SaveIssueStatus {
            name: name.into(),
            color: "#fff".into(),
            category,
        };
        let open = IssueStatus::create(
            &pool,
            project.id,
            &status("Open", IssueStatusCategory::Started),
        )
        .await
        .unwrap();
        let done = IssueStatus::create(
            &pool,
            project.id,
            &status("Done", IssueStatusCategory::Completed),
        )
        .await
        .unwrap();
        let mut request = SyncGitHubIssueRequest {
            repository: "Acme/Repo".into(),
            number: 42,
            open_status_id: open.id,
            closed_status_id: done.id,
            write_back: None,
        };
        let remote = FixtureRemote(std::sync::Mutex::new((
            IssueSyncFields {
                title: "Original".into(),
                description: Some("Body".into()),
                closed: false,
            },
            0,
        )));

        let imported = sync(&pool, project.id, &request, &remote).await.unwrap();
        assert_eq!(imported.outcome, IssueSyncOutcome::Imported);
        assert_eq!(imported.issue.external_id.as_deref(), Some("acme/repo#42"));
        let id = imported.issue.id;
        let unchanged = sync(&pool, project.id, &request, &remote).await.unwrap();
        assert_eq!(unchanged.outcome, IssueSyncOutcome::Unchanged);
        assert_eq!(unchanged.issue.id, id);
        assert_eq!(unchanged.issue.updated_at, imported.issue.updated_at);

        Issue::update(
            &pool,
            project.id,
            id,
            &UpdateIssue {
                title: Some("Local title".into()),
                description: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        remote.0.lock().unwrap().0.closed = true;
        let pending = sync(&pool, project.id, &request, &remote).await.unwrap();
        assert_eq!(pending.outcome, IssueSyncOutcome::PendingWrite);
        assert_eq!(pending.issue.status_id, done.id);
        assert_eq!(pending.issue.title, "Local title");
        assert_eq!(remote.0.lock().unwrap().1, 0);

        request.write_back = Some(true);
        let synced = sync(&pool, project.id, &request, &remote).await.unwrap();
        assert_eq!(synced.outcome, IssueSyncOutcome::Synced);
        assert_eq!(remote.0.lock().unwrap().0.title, "Local title");
        assert!(remote.0.lock().unwrap().0.description.is_none());
        assert_eq!(
            sync(&pool, project.id, &request, &remote)
                .await
                .unwrap()
                .outcome,
            IssueSyncOutcome::Unchanged
        );
        assert_eq!(remote.0.lock().unwrap().1, 1);

        Issue::update(
            &pool,
            project.id,
            id,
            &UpdateIssue {
                title: Some("Local conflict".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        remote.0.lock().unwrap().0.title = "Remote conflict".into();
        let conflict = sync(&pool, project.id, &request, &remote).await.unwrap();
        assert_eq!(conflict.outcome, IssueSyncOutcome::Conflict);
        assert_eq!(conflict.fields, vec![IssueSyncField::Title]);
        assert_eq!(
            Issue::get(&pool, project.id, id).await.unwrap().title,
            "Local conflict"
        );
        let (remote_state, writes) = &*remote.0.lock().unwrap();
        assert_eq!(remote_state.title, "Remote conflict");
        assert_eq!(*writes, 1);
    }
}
