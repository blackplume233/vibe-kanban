use std::collections::HashSet;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool, types::Json};
use ts_rs::TS;
use uuid::Uuid;

use super::issue::{IssueError, require_project, require_row};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum IssueStatusCategory {
    Backlog,
    Unstarted,
    Started,
    Completed,
    Canceled,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct IssueStatus {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub color: String,
    pub category: IssueStatusCategory,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SaveIssueStatus {
    pub name: String,
    pub color: String,
    pub category: IssueStatusCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ReorderIssueStatuses {
    pub status_ids: Vec<Uuid>,
}

impl IssueStatus {
    pub async fn list(pool: &SqlitePool, project_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM project_statuses WHERE project_id = ? ORDER BY sort_order, id",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await
    }

    pub async fn get(pool: &SqlitePool, project_id: Uuid, id: Uuid) -> Result<Self, sqlx::Error> {
        sqlx::query_as("SELECT * FROM project_statuses WHERE project_id = ? AND id = ?")
            .bind(project_id)
            .bind(id)
            .fetch_one(pool)
            .await
    }

    pub async fn create(
        pool: &SqlitePool,
        project_id: Uuid,
        payload: &SaveIssueStatus,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO project_statuses (id, project_id, name, color, category, sort_order)
            VALUES (?, ?, ?, ?, ?, (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM project_statuses
                WHERE project_id = ?)) RETURNING *",
        )
        .bind(Uuid::new_v4())
        .bind(project_id)
        .bind(payload.name.trim())
        .bind(&payload.color)
        .bind(payload.category)
        .bind(project_id)
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        pool: &SqlitePool,
        project_id: Uuid,
        id: Uuid,
        payload: &SaveIssueStatus,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "UPDATE project_statuses SET name = ?, color = ?, category = ?
            WHERE project_id = ? AND id = ? RETURNING *",
        )
        .bind(payload.name.trim())
        .bind(&payload.color)
        .bind(payload.category)
        .bind(project_id)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn delete(pool: &SqlitePool, project_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        require_row(
            sqlx::query("DELETE FROM project_statuses WHERE project_id = ? AND id = ?")
                .bind(project_id)
                .bind(id)
                .execute(pool)
                .await?,
        )
    }

    /// Renumber the whole project from zero. Partial or duplicate lists cannot leave tied positions.
    pub async fn reorder(
        pool: &SqlitePool,
        project_id: Uuid,
        status_ids: &[Uuid],
    ) -> Result<Vec<Self>, IssueError> {
        require_project(pool, project_id).await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut existing: HashSet<Uuid> =
            sqlx::query_scalar("SELECT id FROM project_statuses WHERE project_id = ?")
                .bind(project_id)
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .collect();
        if existing.len() != status_ids.len() || !status_ids.iter().all(|id| existing.remove(id)) {
            return Err(IssueError::Invalid(
                "Provide every status ID in this project exactly once",
            ));
        }
        let mut ordered = Vec::with_capacity(status_ids.len());
        for (position, id) in status_ids.iter().enumerate() {
            let position = i32::try_from(position)
                .map_err(|_| IssueError::Invalid("Too many statuses to reorder"))?;
            ordered.push(
                sqlx::query_as(
                    "UPDATE project_statuses SET sort_order = ?
                WHERE project_id = ? AND id = ? RETURNING *",
                )
                .bind(position)
                .bind(project_id)
                .bind(id)
                .fetch_one(&mut *tx)
                .await?,
            );
        }
        tx.commit().await?;
        Ok(ordered)
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct IssueTag {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SaveIssueTag {
    pub name: String,
    pub color: String,
}

impl IssueTag {
    pub async fn list(pool: &SqlitePool, project_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM issue_labels WHERE project_id = ? ORDER BY name")
            .bind(project_id)
            .fetch_all(pool)
            .await
    }

    pub async fn get(pool: &SqlitePool, project_id: Uuid, id: Uuid) -> Result<Self, sqlx::Error> {
        sqlx::query_as("SELECT * FROM issue_labels WHERE project_id = ? AND id = ?")
            .bind(project_id)
            .bind(id)
            .fetch_one(pool)
            .await
    }

    pub async fn create(
        pool: &SqlitePool,
        project_id: Uuid,
        payload: &SaveIssueTag,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO issue_labels (id, project_id, name, color) VALUES (?, ?, ?, ?)
            RETURNING *",
        )
        .bind(Uuid::new_v4())
        .bind(project_id)
        .bind(payload.name.trim())
        .bind(&payload.color)
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        pool: &SqlitePool,
        project_id: Uuid,
        id: Uuid,
        payload: &SaveIssueTag,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "UPDATE issue_labels SET name = ?, color = ?
            WHERE project_id = ? AND id = ? RETURNING *",
        )
        .bind(payload.name.trim())
        .bind(&payload.color)
        .bind(project_id)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn delete(pool: &SqlitePool, project_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        require_row(
            sqlx::query("DELETE FROM issue_labels WHERE project_id = ? AND id = ?")
                .bind(project_id)
                .bind(id)
                .execute(pool)
                .await?,
        )
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct IssueCycle {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    #[ts(type = "string")]
    pub start_date: NaiveDate,
    #[ts(type = "string")]
    pub end_date: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SaveIssueCycle {
    pub name: String,
    #[ts(type = "string")]
    pub start_date: NaiveDate,
    #[ts(type = "string")]
    pub end_date: NaiveDate,
}

impl IssueCycle {
    pub async fn list(pool: &SqlitePool, project_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM issue_cycles WHERE project_id = ? ORDER BY start_date, name")
            .bind(project_id)
            .fetch_all(pool)
            .await
    }

    pub async fn get(pool: &SqlitePool, project_id: Uuid, id: Uuid) -> Result<Self, sqlx::Error> {
        sqlx::query_as("SELECT * FROM issue_cycles WHERE project_id = ? AND id = ?")
            .bind(project_id)
            .bind(id)
            .fetch_one(pool)
            .await
    }

    pub async fn create(
        pool: &SqlitePool,
        project_id: Uuid,
        payload: &SaveIssueCycle,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO issue_cycles (id, project_id, name, start_date, end_date)
            VALUES (?, ?, ?, ?, ?) RETURNING *",
        )
        .bind(Uuid::new_v4())
        .bind(project_id)
        .bind(payload.name.trim())
        .bind(payload.start_date)
        .bind(payload.end_date)
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        pool: &SqlitePool,
        project_id: Uuid,
        id: Uuid,
        payload: &SaveIssueCycle,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "UPDATE issue_cycles SET name = ?, start_date = ?, end_date = ?
            WHERE project_id = ? AND id = ? RETURNING *",
        )
        .bind(payload.name.trim())
        .bind(payload.start_date)
        .bind(payload.end_date)
        .bind(project_id)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn delete(pool: &SqlitePool, project_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        require_row(
            sqlx::query("DELETE FROM issue_cycles WHERE project_id = ? AND id = ?")
                .bind(project_id)
                .bind(id)
                .execute(pool)
                .await?,
        )
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct IssueSavedView {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    #[sqlx(json)]
    #[ts(type = "Record<string, unknown>")]
    pub filter: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SaveIssueView {
    pub name: String,
    #[ts(type = "Record<string, unknown>")]
    pub filter: Value,
}

impl IssueSavedView {
    pub async fn list(pool: &SqlitePool, project_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM issue_saved_views WHERE project_id = ? ORDER BY name")
            .bind(project_id)
            .fetch_all(pool)
            .await
    }

    pub async fn get(pool: &SqlitePool, project_id: Uuid, id: Uuid) -> Result<Self, sqlx::Error> {
        sqlx::query_as("SELECT * FROM issue_saved_views WHERE project_id = ? AND id = ?")
            .bind(project_id)
            .bind(id)
            .fetch_one(pool)
            .await
    }

    pub async fn create(
        pool: &SqlitePool,
        project_id: Uuid,
        payload: &SaveIssueView,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO issue_saved_views (id, project_id, name, filter) VALUES (?, ?, ?, ?)
            RETURNING *",
        )
        .bind(Uuid::new_v4())
        .bind(project_id)
        .bind(payload.name.trim())
        .bind(Json(&payload.filter))
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        pool: &SqlitePool,
        project_id: Uuid,
        id: Uuid,
        payload: &SaveIssueView,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "UPDATE issue_saved_views SET name = ?, filter = ?
            WHERE project_id = ? AND id = ? RETURNING *",
        )
        .bind(payload.name.trim())
        .bind(Json(&payload.filter))
        .bind(project_id)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn delete(pool: &SqlitePool, project_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        require_row(
            sqlx::query("DELETE FROM issue_saved_views WHERE project_id = ? AND id = ?")
                .bind(project_id)
                .bind(id)
                .execute(pool)
                .await?,
        )
    }
}
