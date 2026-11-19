use sqlx::{FromRow, SqliteConnection, SqlitePool};
use uuid::Uuid;

use super::{
    issue::{CreateIssue, Issue, UpdateIssue},
    issue_board::IssueStatusCategory,
};

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct IssueSyncFields {
    pub title: String,
    pub description: Option<String>,
    pub closed: bool,
}

impl IssueSyncFields {
    pub fn is_closed(category: IssueStatusCategory) -> bool {
        matches!(
            category,
            IssueStatusCategory::Completed | IssueStatusCategory::Canceled
        )
    }
}

#[derive(Debug, PartialEq, Eq, FromRow)]
pub struct GitHubIssueBaseline {
    pub remote_issue_id: i64,
    #[sqlx(flatten)]
    pub fields: IssueSyncFields,
}

#[derive(Debug)]
pub struct LinkedGitHubIssue {
    pub issue: Issue,
    pub fields: IssueSyncFields,
    pub baseline: Option<GitHubIssueBaseline>,
}

#[derive(Debug, thiserror::Error)]
pub enum IssueSyncStoreError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("The local issue or its GitHub link changed during synchronization")]
    ConcurrentChange,
    #[error("More than one local issue uses this GitHub source")]
    DuplicateSource,
}

pub struct GitHubIssueSyncUpdate<'a> {
    pub external_id: &'a str,
    pub external_url: &'a str,
    pub remote_issue_id: i64,
    pub remote: &'a IssueSyncFields,
    pub local: &'a IssueSyncFields,
    pub status_id: Uuid,
}

impl LinkedGitHubIssue {
    pub async fn load(
        pool: &SqlitePool,
        project_id: Uuid,
        external_id: &str,
    ) -> Result<Option<Self>, IssueSyncStoreError> {
        let mut connection = pool.acquire().await?;
        Self::load_in(&mut connection, project_id, external_id).await
    }

    async fn load_in(
        connection: &mut SqliteConnection,
        project_id: Uuid,
        external_id: &str,
    ) -> Result<Option<Self>, IssueSyncStoreError> {
        let mut issues = Issue::find_github_issues(connection, project_id, external_id).await?;
        if issues.len() > 1 {
            return Err(IssueSyncStoreError::DuplicateSource);
        }
        let Some(issue) = issues.pop() else {
            return Ok(None);
        };
        let category = status_category(connection, project_id, issue.status_id).await?;
        let fields = IssueSyncFields {
            title: issue.title.clone(),
            description: issue
                .description
                .as_ref()
                .filter(|text| !text.is_empty())
                .cloned(),
            closed: IssueSyncFields::is_closed(category),
        };
        let baseline = sqlx::query_as("SELECT remote_issue_id, title, description, closed FROM github_issue_sync WHERE issue_id = ? AND external_id = ?")
            .bind(issue.id)
            .bind(external_id)
            .fetch_optional(connection)
            .await?;
        Ok(Some(Self {
            issue,
            fields,
            baseline,
        }))
    }

    pub fn matches(&self, other: &Self) -> bool {
        self.issue.id == other.issue.id
            && self.issue.status_id == other.issue.status_id
            && self.issue.external_id == other.issue.external_id
            && self.issue.external_url == other.issue.external_url
            && self.fields == other.fields
            && self.baseline == other.baseline
    }

    pub async fn save(
        pool: &SqlitePool,
        project_id: Uuid,
        expected: Option<&Self>,
        update: GitHubIssueSyncUpdate<'_>,
    ) -> Result<Issue, IssueSyncStoreError> {
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let current = Self::load_in(&mut tx, project_id, update.external_id).await?;
        if !match (expected, &current) {
            (None, None) => true,
            (Some(expected), Some(current)) => expected.matches(current),
            _ => false,
        } {
            return Err(IssueSyncStoreError::ConcurrentChange);
        }
        let category = status_category(&mut tx, project_id, update.status_id).await?;
        if IssueSyncFields::is_closed(category) != update.local.closed {
            return Err(IssueSyncStoreError::ConcurrentChange);
        }

        let issue = if let Some(current) = current {
            let issue = current.issue;
            let changes = UpdateIssue {
                title: (current.fields.title != update.local.title)
                    .then(|| update.local.title.clone()),
                description: (current.fields.description != update.local.description)
                    .then(|| update.local.description.clone()),
                status_id: (issue.status_id != update.status_id).then_some(update.status_id),
                external_id: (issue.external_id.as_deref() != Some(update.external_id))
                    .then(|| Some(update.external_id.to_owned())),
                external_url: (issue.external_url.as_deref() != Some(update.external_url))
                    .then(|| Some(update.external_url.to_owned())),
                ..Default::default()
            };
            if changes.title.is_none()
                && changes.description.is_none()
                && changes.status_id.is_none()
                && changes.external_id.is_none()
                && changes.external_url.is_none()
            {
                issue
            } else {
                Issue::update_in(&mut tx, project_id, issue.id, &changes).await?
            }
        } else {
            Issue::create_in(
                &mut tx,
                project_id,
                &CreateIssue {
                    title: update.local.title.clone(),
                    status_id: update.status_id,
                    description: update.local.description.clone(),
                    priority: None,
                    start_date: None,
                    target_date: None,
                    sort_order: None,
                    parent_issue_id: None,
                    cycle_id: None,
                    triage: None,
                    extension_metadata: None,
                    external_provider: Some("github".into()),
                    external_id: Some(update.external_id.to_owned()),
                    external_url: Some(update.external_url.to_owned()),
                    tag_ids: None,
                },
            )
            .await?
        };
        if !expected
            .and_then(|issue| issue.baseline.as_ref())
            .is_some_and(|baseline| {
                baseline.remote_issue_id == update.remote_issue_id
                    && baseline.fields == *update.remote
            })
        {
            sqlx::query("INSERT INTO github_issue_sync (issue_id, external_id, remote_issue_id, title, description, closed)
                VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(issue_id) DO UPDATE SET external_id = excluded.external_id,
                remote_issue_id = excluded.remote_issue_id, title = excluded.title, description = excluded.description, closed = excluded.closed")
                .bind(issue.id)
                .bind(update.external_id)
                .bind(update.remote_issue_id)
                .bind(&update.remote.title)
                .bind(&update.remote.description)
                .bind(update.remote.closed)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(issue)
    }
}

async fn status_category(
    connection: &mut SqliteConnection,
    project_id: Uuid,
    status_id: Uuid,
) -> Result<IssueStatusCategory, sqlx::Error> {
    sqlx::query_scalar("SELECT category FROM project_statuses WHERE project_id = ? AND id = ?")
        .bind(project_id)
        .bind(status_id)
        .fetch_one(connection)
        .await
}
