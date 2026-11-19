use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::rust::double_option;
use sqlx::{FromRow, QueryBuilder, Sqlite, SqliteExecutor, SqlitePool, types::Json};
use ts_rs::TS;
use uuid::Uuid;

use super::{issue_board::IssueStatusCategory, project::Project};

#[derive(Debug, thiserror::Error)]
pub enum IssueError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Invalid(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum IssuePriority {
    Urgent,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct Issue {
    pub id: Uuid,
    pub project_id: Uuid,
    #[ts(type = "number")]
    pub issue_number: i64,
    pub simple_id: String,
    pub status_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<IssuePriority>,
    #[ts(type = "string | null")]
    pub start_date: Option<NaiveDate>,
    #[ts(type = "string | null")]
    pub target_date: Option<NaiveDate>,
    pub sort_order: f64,
    pub parent_issue_id: Option<Uuid>,
    pub cycle_id: Option<Uuid>,
    pub triage: bool,
    #[sqlx(json)]
    #[ts(type = "unknown")]
    pub extension_metadata: Value,
    pub external_provider: Option<String>,
    pub external_id: Option<String>,
    pub external_url: Option<String>,
    #[sqlx(json)]
    pub tag_ids: Vec<Uuid>,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "Date")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateIssue {
    pub title: String,
    pub status_id: Uuid,
    #[ts(optional)]
    pub description: Option<String>,
    #[ts(optional)]
    pub priority: Option<IssuePriority>,
    #[ts(optional, type = "string")]
    pub start_date: Option<NaiveDate>,
    #[ts(optional, type = "string")]
    pub target_date: Option<NaiveDate>,
    #[ts(optional)]
    pub sort_order: Option<f64>,
    #[ts(optional)]
    pub parent_issue_id: Option<Uuid>,
    #[ts(optional)]
    pub cycle_id: Option<Uuid>,
    #[ts(optional)]
    pub triage: Option<bool>,
    #[ts(optional, type = "unknown")]
    pub extension_metadata: Option<Value>,
    #[ts(optional)]
    pub external_provider: Option<String>,
    #[ts(optional)]
    pub external_id: Option<String>,
    #[ts(optional)]
    pub external_url: Option<String>,
    #[ts(optional)]
    pub tag_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct UpdateIssue {
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub status_id: Option<Uuid>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, type = "string | null")]
    pub description: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, type = "IssuePriority | null")]
    pub priority: Option<Option<IssuePriority>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, type = "string | null")]
    pub start_date: Option<Option<NaiveDate>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, type = "string | null")]
    pub target_date: Option<Option<NaiveDate>>,
    #[ts(optional)]
    pub sort_order: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, type = "string | null")]
    pub parent_issue_id: Option<Option<Uuid>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, type = "string | null")]
    pub cycle_id: Option<Option<Uuid>>,
    #[ts(optional)]
    pub triage: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub extension_metadata: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, type = "string | null")]
    pub external_provider: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, type = "string | null")]
    pub external_id: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, type = "string | null")]
    pub external_url: Option<Option<String>>,
    #[ts(optional)]
    pub tag_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct IssueBatchUpdate {
    pub issue_id: Uuid,
    pub changes: UpdateIssue,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct IssueFilter {
    #[ts(optional)]
    pub search: Option<String>,
    #[ts(optional)]
    pub status_id: Option<Uuid>,
    #[ts(optional)]
    pub status_category: Option<IssueStatusCategory>,
    #[ts(optional)]
    pub priority: Option<IssuePriority>,
    #[ts(optional)]
    pub tag_id: Option<Uuid>,
    #[ts(optional)]
    pub parent_issue_id: Option<Uuid>,
    #[ts(optional)]
    pub root_only: Option<bool>,
    #[ts(optional)]
    pub cycle_id: Option<Uuid>,
    #[ts(optional)]
    pub triage: Option<bool>,
    #[ts(optional)]
    pub external_provider: Option<String>,
    #[ts(optional)]
    pub external_id: Option<String>,
    #[ts(optional, type = "string")]
    pub start_on_or_after: Option<NaiveDate>,
    #[ts(optional, type = "string")]
    pub target_on_or_before: Option<NaiveDate>,
}

const SELECT_ISSUE: &str = "SELECT i.*, (SELECT json_group_array(lower(hex(tag_id)))
    FROM issue_tags WHERE issue_id = i.id) AS tag_ids FROM issues i WHERE i.project_id = ";

fn like_pattern(search: &str) -> Option<String> {
    let term = search.trim();
    if term.is_empty() {
        return None;
    }
    let mut pattern = String::with_capacity(term.len() + 2);
    pattern.push('%');
    for c in term.chars() {
        if matches!(c, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    Some(pattern)
}

impl Issue {
    pub async fn list(
        pool: &SqlitePool,
        project_id: Uuid,
        filter: &IssueFilter,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let pattern = filter.search.as_deref().and_then(like_pattern);
        let mut query = QueryBuilder::<Sqlite>::new(SELECT_ISSUE);
        query.push_bind(project_id);
        for (column, value) in [
            ("status_id", filter.status_id),
            ("parent_issue_id", filter.parent_issue_id),
            ("cycle_id", filter.cycle_id),
        ] {
            if let Some(value) = value {
                query
                    .push(" AND i.")
                    .push(column)
                    .push(" = ")
                    .push_bind(value);
            }
        }
        if let Some(category) = filter.status_category {
            query.push(" AND EXISTS (SELECT 1 FROM project_statuses s WHERE s.id = i.status_id AND s.category = ")
                .push_bind(category).push(")");
        }
        if let Some(priority) = filter.priority {
            query.push(" AND i.priority = ").push_bind(priority);
        }
        if let Some(tag_id) = filter.tag_id {
            query.push(" AND EXISTS (SELECT 1 FROM issue_tags t WHERE t.issue_id = i.id AND t.tag_id = ")
                .push_bind(tag_id).push(")");
        }
        if filter.root_only == Some(true) {
            query.push(" AND i.parent_issue_id IS NULL");
        }
        if let Some(triage) = filter.triage {
            query.push(" AND i.triage = ").push_bind(triage);
        }
        if let Some(provider) = &filter.external_provider {
            query
                .push(" AND i.external_provider = ")
                .push_bind(provider);
        }
        if let Some(id) = &filter.external_id {
            query.push(" AND i.external_id = ").push_bind(id);
        }
        if let Some(date) = filter.start_on_or_after {
            query.push(" AND i.start_date >= ").push_bind(date);
        }
        if let Some(date) = filter.target_on_or_before {
            query.push(" AND i.target_date <= ").push_bind(date);
        }
        if let Some(pattern) = pattern.as_deref() {
            query
                .push(" AND (i.title LIKE ")
                .push_bind(pattern)
                .push(" ESCAPE '\\' OR i.description LIKE ")
                .push_bind(pattern)
                .push(" ESCAPE '\\' OR i.simple_id LIKE ")
                .push_bind(pattern)
                .push(" ESCAPE '\\')");
        }
        query.push(" ORDER BY i.sort_order, i.issue_number");
        query.build_query_as().fetch_all(pool).await
    }

    pub async fn exists(
        pool: &SqlitePool,
        project_id: Uuid,
        id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM issues WHERE project_id = ? AND id = ?)")
            .bind(project_id)
            .bind(id)
            .fetch_one(pool)
            .await
    }

    pub async fn get(
        executor: impl SqliteExecutor<'_>,
        project_id: Uuid,
        id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        let mut query = QueryBuilder::<Sqlite>::new(SELECT_ISSUE);
        query
            .push_bind(project_id)
            .push(" AND i.id = ")
            .push_bind(id);
        query.build_query_as().fetch_one(executor).await
    }

    pub async fn create(
        pool: &SqlitePool,
        project_id: Uuid,
        payload: &CreateIssue,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        let mut tx = pool.begin().await?;
        sqlx::query(
            "INSERT INTO issues (id, project_id, status_id, title, description, priority,
                start_date, target_date, sort_order, parent_issue_id, cycle_id, triage,
                extension_metadata, external_provider, external_id, external_url)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, COALESCE(?, (SELECT MAX(sort_order) + 1
                FROM issues WHERE project_id = ?), 0), ?, ?, ?, COALESCE(?, '{}'), ?, ?, ?)",
        )
        .bind(id)
        .bind(project_id)
        .bind(payload.status_id)
        .bind(payload.title.trim())
        .bind(payload.description.as_deref())
        .bind(payload.priority)
        .bind(payload.start_date)
        .bind(payload.target_date)
        .bind(payload.sort_order)
        .bind(project_id)
        .bind(payload.parent_issue_id)
        .bind(payload.cycle_id)
        .bind(payload.triage.unwrap_or(false))
        .bind(payload.extension_metadata.as_ref().map(Json))
        .bind(payload.external_provider.as_deref())
        .bind(payload.external_id.as_deref())
        .bind(payload.external_url.as_deref())
        .execute(&mut *tx)
        .await?;
        if let Some(tags) = &payload.tag_ids {
            Self::replace_tags(&mut tx, project_id, id, tags).await?;
        }
        let issue = Self::get(&mut *tx, project_id, id).await?;
        tx.commit().await?;
        Ok(issue)
    }

    pub async fn update(
        pool: &SqlitePool,
        project_id: Uuid,
        id: Uuid,
        payload: &UpdateIssue,
    ) -> Result<Self, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let issue = Self::update_in(&mut tx, project_id, id, payload).await?;
        tx.commit().await?;
        Ok(issue)
    }

    pub async fn update_batch(
        pool: &SqlitePool,
        project_id: Uuid,
        updates: &[IssueBatchUpdate],
    ) -> Result<Vec<Self>, IssueError> {
        let mut seen = HashSet::with_capacity(updates.len());
        if !updates.iter().all(|update| seen.insert(update.issue_id)) {
            return Err(IssueError::Invalid(
                "Each issue may only appear once in a batch",
            ));
        }
        let mut issues = Vec::with_capacity(updates.len());
        let mut tx = pool.begin().await?;
        for update in updates {
            issues.push(
                Self::update_in(&mut tx, project_id, update.issue_id, &update.changes).await?,
            );
        }
        tx.commit().await?;
        Ok(issues)
    }

    async fn update_in(
        conn: &mut sqlx::SqliteConnection,
        project_id: Uuid,
        id: Uuid,
        payload: &UpdateIssue,
    ) -> Result<Self, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE issues SET
            title = COALESCE(?, title), status_id = COALESCE(?, status_id),
            description = CASE WHEN ? THEN ? ELSE description END,
            priority = CASE WHEN ? THEN ? ELSE priority END,
            start_date = CASE WHEN ? THEN ? ELSE start_date END,
            target_date = CASE WHEN ? THEN ? ELSE target_date END,
            sort_order = COALESCE(?, sort_order),
            parent_issue_id = CASE WHEN ? THEN ? ELSE parent_issue_id END,
            cycle_id = CASE WHEN ? THEN ? ELSE cycle_id END,
            triage = COALESCE(?, triage), extension_metadata = COALESCE(?, extension_metadata),
            external_provider = CASE WHEN ? THEN ? ELSE external_provider END,
            external_id = CASE WHEN ? THEN ? ELSE external_id END,
            external_url = CASE WHEN ? THEN ? ELSE external_url END,
            updated_at = datetime('now', 'subsec') WHERE project_id = ? AND id = ?",
        )
        .bind(payload.title.as_deref().map(str::trim))
        .bind(payload.status_id)
        .bind(payload.description.is_some())
        .bind(payload.description.as_ref().and_then(Option::as_deref))
        .bind(payload.priority.is_some())
        .bind(payload.priority.flatten())
        .bind(payload.start_date.is_some())
        .bind(payload.start_date.flatten())
        .bind(payload.target_date.is_some())
        .bind(payload.target_date.flatten())
        .bind(payload.sort_order)
        .bind(payload.parent_issue_id.is_some())
        .bind(payload.parent_issue_id.flatten())
        .bind(payload.cycle_id.is_some())
        .bind(payload.cycle_id.flatten())
        .bind(payload.triage)
        .bind(payload.extension_metadata.as_ref().map(Json))
        .bind(payload.external_provider.is_some())
        .bind(
            payload
                .external_provider
                .as_ref()
                .and_then(Option::as_deref),
        )
        .bind(payload.external_id.is_some())
        .bind(payload.external_id.as_ref().and_then(Option::as_deref))
        .bind(payload.external_url.is_some())
        .bind(payload.external_url.as_ref().and_then(Option::as_deref))
        .bind(project_id)
        .bind(id)
        .execute(&mut *conn)
        .await?;
        require_row(result)?;
        if let Some(tags) = &payload.tag_ids {
            Self::replace_tags(conn, project_id, id, tags).await?;
        }
        Self::get(conn, project_id, id).await
    }

    async fn replace_tags(
        conn: &mut sqlx::SqliteConnection,
        project_id: Uuid,
        id: Uuid,
        tags: &[Uuid],
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM issue_tags WHERE issue_id = ?")
            .bind(id)
            .execute(&mut *conn)
            .await?;
        for tag in tags {
            sqlx::query(
                "INSERT INTO issue_tags (project_id, issue_id, tag_id) VALUES (?, ?, ?)
                ON CONFLICT(issue_id, tag_id) DO NOTHING",
            )
            .bind(project_id)
            .bind(id)
            .bind(tag)
            .execute(&mut *conn)
            .await?;
        }
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, project_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        require_row(
            sqlx::query("DELETE FROM issues WHERE project_id = ? AND id = ?")
                .bind(project_id)
                .bind(id)
                .execute(pool)
                .await?,
        )
    }
}

pub(super) fn require_row(result: sqlx::sqlite::SqliteQueryResult) -> Result<(), sqlx::Error> {
    if result.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

pub async fn require_project(pool: &SqlitePool, project_id: Uuid) -> Result<(), sqlx::Error> {
    Project::exists(pool, project_id)
        .await?
        .then_some(())
        .ok_or(sqlx::Error::RowNotFound)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::models::{
        issue_board::{
            IssueCycle, IssueSavedView, IssueStatus, IssueTag, SaveIssueCycle, SaveIssueStatus,
            SaveIssueTag, SaveIssueView,
        },
        issue_links::{
            CreateIssueComment, CreateIssueRelationship, IssueComment, IssueRelationship,
            IssueRelationshipType,
        },
    };

    async fn setup() -> (SqlitePool, Uuid, IssueStatus) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::run_migrations(&pool).await.unwrap();
        let project = Project::create(&pool, "Local board").await.unwrap();
        let status = status(&pool, project.id, "Backlog", IssueStatusCategory::Backlog).await;
        (pool, project.id, status)
    }

    async fn status(
        pool: &SqlitePool,
        project: Uuid,
        name: &str,
        category: IssueStatusCategory,
    ) -> IssueStatus {
        IssueStatus::create(
            pool,
            project,
            &SaveIssueStatus {
                name: name.into(),
                color: "#223344".into(),
                category,
            },
        )
        .await
        .unwrap()
    }

    fn issue_data(status: Uuid, title: &str) -> CreateIssue {
        serde_json::from_value(json!({ "status_id": status, "title": title })).unwrap()
    }

    #[sqlx::test]
    async fn migrates_existing_projects_without_changing_them() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let mut migrator = sqlx::migrate!("./migrations");
        let before_issues = migrator
            .iter()
            .take_while(|migration| migration.version < 20261119100000)
            .cloned()
            .collect();
        migrator.migrations = std::borrow::Cow::Owned(before_issues);
        migrator.run(&pool).await.unwrap();
        let project = Project::create(&pool, "Existing project").await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        assert_eq!(Project::find_all(&pool).await.unwrap()[0].id, project.id);
        let status = status(
            &pool,
            project.id,
            "First status",
            IssueStatusCategory::Backlog,
        )
        .await;
        assert_eq!(
            Issue::create(&pool, project.id, &issue_data(status.id, "First issue"))
                .await
                .unwrap()
                .simple_id,
            "VO-1"
        );
    }

    #[sqlx::test]
    async fn crud_search_fields_and_external_identity() {
        let (pool, project, backlog) = setup().await;
        let tag = IssueTag::create(
            &pool,
            project,
            &SaveIssueTag {
                name: "bug".into(),
                color: "red".into(),
            },
        )
        .await
        .unwrap();
        let data = serde_json::from_value(json!({
            "status_id": backlog.id, "title": "  Repair renderer  ", "description": "Battery at 90%_ now",
            "priority": "high", "start_date": "2026-11-19", "target_date": "2026-11-21",
            "extension_metadata": { "attempt": 1 }, "triage": true, "external_provider": "github",
            "external_id": "owner/repo#7", "external_url": "https://example.test/7", "tag_ids": [tag.id]
        })).unwrap();
        let issue = Issue::create(&pool, project, &data).await.unwrap();
        assert_eq!(issue.title, "Repair renderer");
        assert_eq!(issue.tag_ids, [tag.id]);
        assert_eq!(issue.extension_metadata, json!({"attempt": 1}));
        assert!(Issue::create(&pool, project, &data).await.is_err());
        Issue::create(&pool, project, &issue_data(backlog.id, "Another issue"))
            .await
            .unwrap();
        for search in ["rEnDeR", "90%_", "vo-1"] {
            let filter = IssueFilter {
                search: Some(search.into()),
                ..Default::default()
            };
            assert_eq!(
                Issue::list(&pool, project, &filter).await.unwrap()[0].id,
                issue.id
            );
            assert_eq!(Issue::list(&pool, project, &filter).await.unwrap().len(), 1);
        }
        assert!(
            Issue::list(
                &pool,
                project,
                &IssueFilter {
                    search: Some("%missing_".into()),
                    ..Default::default()
                }
            )
            .await
            .unwrap()
            .is_empty()
        );
        let filter = serde_json::from_value(
            json!({"tag_id": tag.id, "status_category": "backlog", "priority": "high",
            "triage": true, "external_provider": "github", "external_id": "owner/repo#7",
            "start_on_or_after": "2026-11-18", "target_on_or_before": "2026-11-22"}),
        )
        .unwrap();
        assert_eq!(Issue::list(&pool, project, &filter).await.unwrap().len(), 1);
        let update =
            serde_json::from_value(json!({"description": null, "tag_ids": [], "triage": false}))
                .unwrap();
        let issue = Issue::update(&pool, project, issue.id, &update)
            .await
            .unwrap();
        assert_eq!(issue.description, None);
        assert_eq!(issue.priority, Some(IssuePriority::High));
        assert!(!issue.triage);
        assert!(issue.tag_ids.is_empty());
        Issue::delete(&pool, project, issue.id).await.unwrap();
        assert!(matches!(
            Issue::get(&pool, project, issue.id).await,
            Err(sqlx::Error::RowNotFound)
        ));
        let newer = Issue::create(&pool, project, &issue_data(backlog.id, "Later issue"))
            .await
            .unwrap();
        assert!(newer.issue_number > issue.issue_number);
    }

    #[sqlx::test]
    async fn reorders_statuses_and_updates_issues_atomically() {
        let (pool, project, backlog) = setup().await;
        let started = status(&pool, project, "Doing", IssueStatusCategory::Started).await;
        let first = Issue::create(&pool, project, &issue_data(backlog.id, "First"))
            .await
            .unwrap();
        let second = Issue::create(&pool, project, &issue_data(backlog.id, "Second"))
            .await
            .unwrap();
        let ordered = IssueStatus::reorder(&pool, project, &[started.id, backlog.id])
            .await
            .unwrap();
        assert_eq!(ordered[0].sort_order, 0);
        assert_eq!(
            IssueStatus::list(&pool, project).await.unwrap()[0].id,
            started.id
        );
        assert!(
            IssueStatus::reorder(&pool, project, &[started.id, started.id])
                .await
                .is_err()
        );
        let tag = IssueTag::create(
            &pool,
            project,
            &SaveIssueTag {
                name: "new".into(),
                color: "red".into(),
            },
        )
        .await
        .unwrap();
        let updates = [
            IssueBatchUpdate {
                issue_id: first.id,
                changes: UpdateIssue {
                    sort_order: Some(2.),
                    ..Default::default()
                },
            },
            IssueBatchUpdate {
                issue_id: second.id,
                changes: UpdateIssue {
                    sort_order: Some(-1.),
                    status_id: Some(started.id),
                    priority: Some(Some(IssuePriority::Urgent)),
                    tag_ids: Some(vec![tag.id]),
                    ..Default::default()
                },
            },
        ];
        Issue::update_batch(&pool, project, &updates).await.unwrap();
        let listed = Issue::list(&pool, project, &IssueFilter::default())
            .await
            .unwrap();
        assert_eq!(
            (listed[0].id, listed[0].status_id, listed[0].priority),
            (second.id, started.id, Some(IssuePriority::Urgent))
        );
        assert_eq!(listed[0].tag_ids, [tag.id]);
        let invalid_batch = [
            IssueBatchUpdate {
                issue_id: first.id,
                changes: UpdateIssue {
                    sort_order: Some(-10.),
                    ..Default::default()
                },
            },
            IssueBatchUpdate {
                issue_id: Uuid::new_v4(),
                changes: UpdateIssue::default(),
            },
        ];
        assert!(
            Issue::update_batch(&pool, project, &invalid_batch)
                .await
                .is_err()
        );
        assert_eq!(
            Issue::get(&pool, project, first.id)
                .await
                .unwrap()
                .sort_order,
            2.
        );
        assert!(
            IssueStatus::delete(&pool, project, started.id)
                .await
                .is_err()
        );
        IssueTag::delete(&pool, project, tag.id).await.unwrap();
        assert!(
            Issue::get(&pool, project, second.id)
                .await
                .unwrap()
                .tag_ids
                .is_empty()
        );
    }

    #[sqlx::test]
    async fn enforces_project_scoping_and_cleans_up_links() {
        let (pool, project, backlog) = setup().await;
        let foreign = Project::create(&pool, "Other board").await.unwrap().id;
        let foreign_status = status(&pool, foreign, "Backlog", IssueStatusCategory::Backlog).await;
        let other = Issue::create(&pool, foreign, &issue_data(foreign_status.id, "Foreign"))
            .await
            .unwrap();
        assert!(
            Issue::create(
                &pool,
                project,
                &issue_data(foreign_status.id, "Wrong project")
            )
            .await
            .is_err()
        );
        let parent = Issue::create(&pool, project, &issue_data(backlog.id, "Parent"))
            .await
            .unwrap();
        let mut child = issue_data(backlog.id, "Child");
        child.parent_issue_id = Some(other.id);
        assert!(Issue::create(&pool, project, &child).await.is_err());
        child.parent_issue_id = Some(parent.id);
        let child = Issue::create(&pool, project, &child).await.unwrap();
        let cycle = UpdateIssue {
            parent_issue_id: Some(Some(child.id)),
            ..Default::default()
        };
        assert!(
            Issue::update(&pool, project, parent.id, &cycle)
                .await
                .is_err()
        );
        let mut link = CreateIssueRelationship {
            related_issue_id: other.id,
            relationship_type: IssueRelationshipType::Related,
        };
        assert!(
            IssueRelationship::create(&pool, project, parent.id, &link)
                .await
                .is_err()
        );
        link.related_issue_id = child.id;
        let relationship = IssueRelationship::create(&pool, project, parent.id, &link)
            .await
            .unwrap();
        link.related_issue_id = parent.id;
        assert!(
            IssueRelationship::create(&pool, project, child.id, &link)
                .await
                .is_err()
        );
        link.relationship_type = IssueRelationshipType::Blocking;
        let blocking = IssueRelationship::create(&pool, project, child.id, &link)
            .await
            .unwrap();
        assert_eq!(
            (blocking.issue_id, blocking.related_issue_id),
            (child.id, parent.id)
        );
        let incoming = IssueRelationship::list(&pool, project, child.id)
            .await
            .unwrap();
        assert!(incoming.iter().any(|row| row.id == relationship.id));
        IssueRelationship::delete(&pool, project, child.id, relationship.id)
            .await
            .unwrap();
        let comment = IssueComment::create(
            &pool,
            project,
            child.id,
            &CreateIssueComment {
                message: "child comment".into(),
                parent_id: None,
            },
        )
        .await
        .unwrap();
        let reply = CreateIssueComment {
            message: "reply".into(),
            parent_id: Some(comment.id),
        };
        assert!(
            IssueComment::create(&pool, project, parent.id, &reply)
                .await
                .is_err()
        );
        let reply = IssueComment::create(&pool, project, child.id, &reply)
            .await
            .unwrap();
        IssueComment::delete(&pool, project, child.id, comment.id)
            .await
            .unwrap();
        let comments = IssueComment::list(&pool, project, child.id).await.unwrap();
        assert_eq!((comments[0].id, comments[0].parent_id), (reply.id, None));
        Issue::delete(&pool, project, parent.id).await.unwrap();
        assert_eq!(
            Issue::get(&pool, project, child.id)
                .await
                .unwrap()
                .parent_issue_id,
            None
        );
        assert!(
            IssueRelationship::list(&pool, project, child.id)
                .await
                .unwrap()
                .is_empty()
        );
        sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(project)
            .execute(&pool)
            .await
            .unwrap();
        assert!(!Issue::exists(&pool, project, child.id).await.unwrap());
    }

    #[sqlx::test]
    async fn round_trips_cycles_and_saved_views() {
        let (pool, project, backlog) = setup().await;
        let mut data: SaveIssueCycle = serde_json::from_value(
            json!({"name": "Sprint", "start_date": "2026-11-19", "end_date": "2026-11-26"}),
        )
        .unwrap();
        let cycle = IssueCycle::create(&pool, project, &data).await.unwrap();
        assert_eq!(
            IssueCycle::list(&pool, project).await.unwrap()[0].id,
            cycle.id
        );
        data.name = "Renamed sprint".into();
        IssueCycle::update(&pool, project, cycle.id, &data)
            .await
            .unwrap();
        assert_eq!(
            IssueCycle::get(&pool, project, cycle.id)
                .await
                .unwrap()
                .name,
            data.name
        );
        let mut issue = issue_data(backlog.id, "Scheduled");
        issue.cycle_id = Some(cycle.id);
        let issue = Issue::create(&pool, project, &issue).await.unwrap();
        let mut data = SaveIssueView {
            name: "Sprint view".into(),
            filter: json!({"cycle_id": cycle.id, "status_id": backlog.id}),
        };
        let view = IssueSavedView::create(&pool, project, &data).await.unwrap();
        let filter = serde_json::from_value(view.filter).unwrap();
        assert_eq!(
            Issue::list(&pool, project, &filter).await.unwrap()[0].id,
            issue.id
        );
        data.filter = json!({"triage": true});
        IssueSavedView::update(&pool, project, view.id, &data)
            .await
            .unwrap();
        assert_eq!(
            IssueSavedView::get(&pool, project, view.id)
                .await
                .unwrap()
                .filter,
            data.filter
        );
        IssueSavedView::delete(&pool, project, view.id)
            .await
            .unwrap();
        assert!(
            IssueSavedView::list(&pool, project)
                .await
                .unwrap()
                .is_empty()
        );
        IssueCycle::delete(&pool, project, cycle.id).await.unwrap();
        assert_eq!(
            Issue::get(&pool, project, issue.id).await.unwrap().cycle_id,
            None
        );
    }
}
