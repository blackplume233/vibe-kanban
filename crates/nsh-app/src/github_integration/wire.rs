use serde::{Deserialize, Serialize};

use crate::{Api, issues_api::Issue};

#[derive(Deserialize)]
pub(super) struct Repository {
    pub id: String,
    pub display_name: String,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum PrStatus {
    Open,
    Merged,
    Closed,
    Unknown,
}

#[derive(Deserialize, PartialEq)]
pub(super) struct PullRequest {
    pub number: i64,
    pub url: String,
    pub title: String,
    pub status: PrStatus,
    pub checks: Option<Checks>,
}

#[derive(Deserialize, PartialEq)]
pub(super) struct Checks {
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
    pub failures: Vec<FailedCheck>,
}

#[derive(Deserialize, PartialEq)]
pub(super) struct FailedCheck {
    pub name: String,
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(super) enum FixCi {
    Scheduled,
    Queued,
}

#[derive(Deserialize)]
struct Session {
    id: String,
}

#[derive(Deserialize)]
struct Draft {
    payload: DraftPayload,
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
enum DraftPayload {
    DraftFollowUp { message: String },
}

#[derive(Serialize)]
pub(super) struct SyncIssue<'a> {
    pub repository: &'a str,
    pub number: u32,
    pub open_status_id: &'a str,
    pub closed_status_id: &'a str,
    pub write_back: bool,
}

#[derive(Deserialize)]
pub(super) struct SyncResult {
    pub issue: Issue,
    pub outcome: SyncOutcome,
    pub fields: Vec<SyncField>,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum SyncOutcome {
    Imported,
    Synced,
    Unchanged,
    PendingWrite,
    Conflict,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SyncField {
    Title,
    Description,
    State,
    RemoteIdentity,
}

impl SyncField {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Title => "标题",
            Self::Description => "描述",
            Self::State => "状态",
            Self::RemoteIdentity => "GitHub 议题标识",
        }
    }
}

impl Api {
    pub(super) async fn github_workspace_repos(
        &self,
        workspace: &str,
    ) -> Result<Vec<Repository>, String> {
        self.request("GET", &format!("/api/workspaces/{workspace}/repos"), None)
            .await
    }

    pub(super) async fn github_pull_request(
        &self,
        workspace: &str,
        repo: &str,
    ) -> Result<PullRequest, String> {
        self.request(
            "GET",
            &format!("/api/workspaces/{workspace}/pull-requests/detail?repo_id={repo}"),
            None,
        )
        .await
    }

    pub(super) async fn github_fix_ci(&self, workspace: &str, repo: &str) -> Result<FixCi, String> {
        self.request(
            "POST",
            &format!("/api/workspaces/{workspace}/pull-requests/fix-ci?repo_id={repo}"),
            None,
        )
        .await
    }

    pub(super) async fn github_follow_up_draft(
        &self,
        workspace: &str,
    ) -> Result<Option<String>, String> {
        let sessions: Vec<Session> = self
            .request(
                "GET",
                &format!("/api/sessions?workspace_id={workspace}"),
                None,
            )
            .await?;
        for session in sessions {
            let path = format!("/api/scratch/DRAFT_FOLLOW_UP/{}", session.id);
            match self.request::<Draft>("GET", &path, None).await {
                Ok(Draft {
                    payload: DraftPayload::DraftFollowUp { message },
                }) => return Ok(Some(message)),
                Err(message) if message == "Scratch not found" => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(None)
    }

    pub(super) async fn github_sync_issue(
        &self,
        project: &str,
        request: SyncIssue<'_>,
    ) -> Result<SyncResult, String> {
        let body = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
        self.request(
            "POST",
            &format!("/api/projects/{project}/github/issues/sync"),
            Some(body),
        )
        .await
    }
}
