use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

use super::scratch::DraftFollowUpData;

#[derive(Debug, Clone, Serialize, Deserialize, TS, FromRow)]
pub struct ScheduledExecution {
    pub execution_process_id: Uuid,
    pub session_id: Uuid,
    pub workspace_id: Uuid,
    pub priority: i32,
    pub queued_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
pub struct ExecutionSchedulerConfig {
    pub max_concurrent: u32,
}

#[derive(Debug, FromRow)]
pub struct StoredFollowUp {
    pub session_id: Uuid,
    pub payload: sqlx::types::Json<DraftFollowUpData>,
    pub queued_at: DateTime<Utc>,
    pub revision: Uuid,
}

impl ScheduledExecution {
    pub async fn enqueue(pool: &SqlitePool, execution_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO execution_queue (execution_process_id, queued_at) VALUES (?, ?)")
            .bind(execution_id)
            .bind(Utc::now())
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn promote_follow_up(
        pool: &SqlitePool,
        id: Uuid,
        draft: &StoredFollowUp,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let removed =
            sqlx::query("DELETE FROM execution_follow_ups WHERE session_id = ? AND revision = ?")
                .bind(draft.session_id)
                .bind(draft.revision)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        if removed == 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query("INSERT INTO execution_queue (execution_process_id, queued_at) VALUES (?, ?)")
            .bind(id)
            .bind(draft.queued_at)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn list(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT q.execution_process_id, p.session_id, s.workspace_id, q.priority, q.queued_at
             FROM execution_queue q
             JOIN execution_processes p ON p.id = q.execution_process_id
             JOIN sessions s ON p.session_id = s.id
             WHERE q.dispatched = 0 AND p.status = 'running'
             ORDER BY q.priority DESC, q.position",
        )
        .fetch_all(pool)
        .await
    }

    /// The scheduler serializes selection and claiming against cancellation.
    pub async fn next(pool: &SqlitePool) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            "WITH active AS (
                 SELECT s.workspace_id FROM execution_queue slot
                 JOIN execution_processes p ON p.id = slot.execution_process_id
                 JOIN sessions s ON p.session_id = s.id
                 WHERE slot.dispatched = 1
             )
             SELECT q.execution_process_id FROM execution_queue q
             JOIN execution_processes p ON p.id = q.execution_process_id
             JOIN sessions s ON p.session_id = s.id
             WHERE q.dispatched = 0 AND p.status = 'running'
               AND (SELECT count(*) FROM active) < (SELECT max_concurrent FROM execution_scheduler_config WHERE id = 1)
               AND NOT EXISTS (SELECT 1 FROM active WHERE workspace_id = s.workspace_id)
               AND NOT EXISTS (
                   SELECT 1 FROM execution_processes c JOIN sessions cs ON cs.id = c.session_id
                   WHERE cs.workspace_id = s.workspace_id AND c.status = 'running' AND c.run_reason = 'cleanupscript'
               )
             ORDER BY q.priority DESC, q.position LIMIT 1",
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn set_priority(
        pool: &SqlitePool,
        id: Uuid,
        priority: i32,
    ) -> Result<bool, sqlx::Error> {
        Ok(sqlx::query("UPDATE execution_queue SET priority = ? WHERE execution_process_id = ? AND dispatched = 0")
            .bind(priority)
            .bind(id)
            .execute(pool)
            .await?
            .rows_affected() != 0)
    }

    pub async fn remove(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
        Ok(
            sqlx::query("DELETE FROM execution_queue WHERE execution_process_id = ?")
                .bind(id)
                .execute(pool)
                .await?
                .rows_affected()
                != 0,
        )
    }

    pub async fn claim(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE execution_queue SET dispatched = 1 WHERE execution_process_id = ? AND dispatched = 0")
            .bind(id).execute(pool).await?;
        Ok(())
    }

    pub async fn is_waiting(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_queue WHERE execution_process_id = ? AND dispatched = 0)")
            .bind(id).fetch_one(pool).await
    }

    pub async fn clear_orphan_slots(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM execution_queue WHERE dispatched = 1 AND execution_process_id IN (SELECT id FROM execution_processes WHERE status != 'running')")
            .execute(pool).await?;
        Ok(())
    }
}

impl ExecutionSchedulerConfig {
    pub async fn get(pool: &SqlitePool) -> Result<Self, sqlx::Error> {
        let max_concurrent = sqlx::query_scalar(
            "SELECT max_concurrent FROM execution_scheduler_config WHERE id = 1",
        )
        .fetch_one(pool)
        .await?;
        Ok(Self { max_concurrent })
    }

    pub async fn save(self, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE execution_scheduler_config SET max_concurrent = ? WHERE id = 1")
            .bind(self.max_concurrent)
            .execute(pool)
            .await?;
        Ok(())
    }
}

impl StoredFollowUp {
    pub async fn save(
        pool: &SqlitePool,
        session_id: Uuid,
        data: DraftFollowUpData,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO execution_follow_ups (session_id, payload, queued_at, revision) VALUES (?, ?, ?, ?)
             ON CONFLICT(session_id) DO UPDATE SET payload = excluded.payload,
                 queued_at = excluded.queued_at, revision = excluded.revision
             RETURNING session_id, payload, queued_at, revision",
        )
        .bind(session_id)
        .bind(sqlx::types::Json(data))
        .bind(Utc::now())
        .bind(Uuid::new_v4())
        .fetch_one(pool)
        .await
    }

    pub async fn get(pool: &SqlitePool, session_id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT session_id, payload, queued_at, revision FROM execution_follow_ups WHERE session_id = ?")
            .bind(session_id)
            .fetch_optional(pool)
            .await
    }

    pub async fn delete(
        pool: &SqlitePool,
        session_id: Uuid,
        revision: Option<Uuid>,
    ) -> Result<bool, sqlx::Error> {
        Ok(sqlx::query("DELETE FROM execution_follow_ups WHERE session_id = ?1 AND (?2 IS NULL OR revision = ?2)")
            .bind(session_id)
            .bind(revision)
            .execute(pool)
            .await?
            .rows_affected() != 0)
    }

    /// Recover drafts whose prerequisite reached a terminal state while the server was down.
    pub async fn recoverable(pool: &SqlitePool) -> Result<Vec<(Uuid, Uuid)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT f.session_id, (
                 SELECT p.id FROM execution_processes p
                 WHERE p.session_id = f.session_id AND p.run_reason IN ('codingagent', 'setupscript', 'cleanupscript')
                   AND p.dropped = 0 ORDER BY p.created_at DESC LIMIT 1
             ) AS process_id
             FROM execution_follow_ups f
             WHERE process_id IS NOT NULL AND NOT EXISTS (
                 SELECT 1 FROM execution_processes r
                 WHERE r.session_id = f.session_id AND r.status = 'running'
                   AND r.run_reason IN ('codingagent', 'setupscript', 'cleanupscript')
             )
             ORDER BY f.queued_at, f.rowid",
        )
        .fetch_all(pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use executors::{executors::BaseCodingAgent, profile::ExecutorConfig};

    async fn seed_execution(pool: &SqlitePool) -> (Uuid, Uuid) {
        let workspace = Uuid::new_v4();
        let session = Uuid::new_v4();
        let execution = Uuid::new_v4();
        sqlx::query("INSERT INTO workspaces (id, branch) VALUES (?, 'test')")
            .bind(workspace)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO sessions (id, workspace_id) VALUES (?, ?)")
            .bind(session)
            .bind(workspace)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO execution_processes (id, session_id, run_reason) VALUES (?, ?, 'codingagent')")
            .bind(execution).bind(session).execute(pool).await.unwrap();
        (session, execution)
    }

    #[sqlx::test]
    async fn waits_at_capacity_then_dispatches_by_priority_and_fifo(pool: SqlitePool) {
        assert_eq!(
            ExecutionSchedulerConfig::get(&pool)
                .await
                .unwrap()
                .max_concurrent,
            4
        );
        ExecutionSchedulerConfig { max_concurrent: 2 }
            .save(&pool)
            .await
            .unwrap();
        let mut ids = Vec::new();
        for _ in 0..5 {
            let (_, id) = seed_execution(&pool).await;
            ScheduledExecution::enqueue(&pool, id).await.unwrap();
            ids.push(id);
        }
        for id in &ids[..2] {
            assert_eq!(ScheduledExecution::next(&pool).await.unwrap(), Some(*id));
            ScheduledExecution::claim(&pool, *id).await.unwrap();
        }
        assert_eq!(ScheduledExecution::next(&pool).await.unwrap(), None);
        assert_eq!(ScheduledExecution::list(&pool).await.unwrap().len(), 3);
        ScheduledExecution::set_priority(&pool, ids[3], 9)
            .await
            .unwrap();
        ScheduledExecution::set_priority(&pool, ids[4], 9)
            .await
            .unwrap();
        ScheduledExecution::remove(&pool, ids[0]).await.unwrap();
        assert_eq!(ScheduledExecution::next(&pool).await.unwrap(), Some(ids[3]));
        ScheduledExecution::remove(&pool, ids[3]).await.unwrap();
        assert_eq!(ScheduledExecution::next(&pool).await.unwrap(), Some(ids[4]));
    }

    #[sqlx::test]
    async fn restart_reclaims_only_dispatched_slots_and_draft_is_promoted_once(pool: SqlitePool) {
        let (_, active) = seed_execution(&pool).await;
        ScheduledExecution::enqueue(&pool, active).await.unwrap();
        ScheduledExecution::claim(&pool, active).await.unwrap();
        let (session, queued) = seed_execution(&pool).await;
        let draft = StoredFollowUp::save(
            &pool,
            session,
            DraftFollowUpData {
                message: "second turn".to_string(),
                executor_config: ExecutorConfig::new(BaseCodingAgent::codex()),
            },
        )
        .await
        .unwrap();
        assert!(
            ScheduledExecution::promote_follow_up(&pool, queued, &draft)
                .await
                .unwrap()
        );
        assert!(
            !ScheduledExecution::promote_follow_up(&pool, queued, &draft)
                .await
                .unwrap()
        );
        assert!(StoredFollowUp::get(&pool, session).await.unwrap().is_none());
        sqlx::query("UPDATE execution_processes SET status = 'failed' WHERE id = ?")
            .bind(active)
            .execute(&pool)
            .await
            .unwrap();
        ScheduledExecution::clear_orphan_slots(&pool).await.unwrap();
        assert_eq!(ScheduledExecution::next(&pool).await.unwrap(), Some(queued));
        assert!(ScheduledExecution::is_waiting(&pool, queued).await.unwrap());
        assert_eq!(ScheduledExecution::list(&pool).await.unwrap().len(), 1);
    }
}
