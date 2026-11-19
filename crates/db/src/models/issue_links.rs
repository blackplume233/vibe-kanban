use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

use super::issue::require_row;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum IssueRelationshipType {
    /// The source issue blocks the target.
    Blocking,
    /// A symmetric link, stored once per pair.
    Related,
    /// The source issue duplicates the target.
    Duplicate,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct IssueRelationship {
    pub id: Uuid,
    pub project_id: Uuid,
    pub issue_id: Uuid,
    pub related_issue_id: Uuid,
    pub relationship_type: IssueRelationshipType,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateIssueRelationship {
    pub related_issue_id: Uuid,
    pub relationship_type: IssueRelationshipType,
}

impl IssueRelationship {
    pub async fn list(
        pool: &SqlitePool,
        project_id: Uuid,
        issue_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM issue_relationships WHERE project_id = ?
            AND (issue_id = ? OR related_issue_id = ?) ORDER BY created_at, id",
        )
        .bind(project_id)
        .bind(issue_id)
        .bind(issue_id)
        .fetch_all(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        project_id: Uuid,
        issue_id: Uuid,
        payload: &CreateIssueRelationship,
    ) -> Result<Self, sqlx::Error> {
        let related = payload.related_issue_id;
        let (source, target) =
            if payload.relationship_type == IssueRelationshipType::Related && related < issue_id {
                (related, issue_id)
            } else {
                (issue_id, related)
            };
        sqlx::query_as(
            "INSERT INTO issue_relationships (id, project_id, issue_id, related_issue_id,
            relationship_type) VALUES (?, ?, ?, ?, ?) RETURNING *",
        )
        .bind(Uuid::new_v4())
        .bind(project_id)
        .bind(source)
        .bind(target)
        .bind(payload.relationship_type)
        .fetch_one(pool)
        .await
    }

    pub async fn delete(
        pool: &SqlitePool,
        project_id: Uuid,
        issue_id: Uuid,
        id: Uuid,
    ) -> Result<(), sqlx::Error> {
        require_row(
            sqlx::query(
                "DELETE FROM issue_relationships WHERE project_id = ? AND id = ?
            AND (issue_id = ? OR related_issue_id = ?)",
            )
            .bind(project_id)
            .bind(id)
            .bind(issue_id)
            .bind(issue_id)
            .execute(pool)
            .await?,
        )
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct IssueComment {
    pub id: Uuid,
    pub project_id: Uuid,
    pub issue_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub message: String,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "Date")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateIssueComment {
    pub message: String,
    #[ts(optional)]
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpdateIssueComment {
    pub message: String,
}

impl IssueComment {
    pub async fn list(
        pool: &SqlitePool,
        project_id: Uuid,
        issue_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM issue_comments WHERE project_id = ? AND issue_id = ?
            ORDER BY created_at, id",
        )
        .bind(project_id)
        .bind(issue_id)
        .fetch_all(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        project_id: Uuid,
        issue_id: Uuid,
        payload: &CreateIssueComment,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO issue_comments (id, project_id, issue_id, parent_id, message)
            VALUES (?, ?, ?, ?, ?) RETURNING *",
        )
        .bind(Uuid::new_v4())
        .bind(project_id)
        .bind(issue_id)
        .bind(payload.parent_id)
        .bind(&payload.message)
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        pool: &SqlitePool,
        project_id: Uuid,
        issue_id: Uuid,
        id: Uuid,
        payload: &UpdateIssueComment,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "UPDATE issue_comments SET message = ?, updated_at = datetime('now', 'subsec')
            WHERE project_id = ? AND issue_id = ? AND id = ? RETURNING *",
        )
        .bind(&payload.message)
        .bind(project_id)
        .bind(issue_id)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn delete(
        pool: &SqlitePool,
        project_id: Uuid,
        issue_id: Uuid,
        id: Uuid,
    ) -> Result<(), sqlx::Error> {
        require_row(
            sqlx::query(
                "DELETE FROM issue_comments WHERE project_id = ? AND issue_id = ? AND id = ?",
            )
            .bind(project_id)
            .bind(issue_id)
            .bind(id)
            .execute(pool)
            .await?,
        )
    }
}
