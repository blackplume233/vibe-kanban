use std::sync::Arc;

use chrono::{DateTime, Utc};
use db::models::{execution_queue::StoredFollowUp, scratch::DraftFollowUpData};
use serde::{Deserialize, Serialize};
use sqlx::{Error, SqlitePool};
use tokio::sync::Notify;
use ts_rs::TS;
use uuid::Uuid;

/// Represents a queued follow-up message for a session
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct QueuedMessage {
    /// The session this message is queued for
    pub session_id: Uuid,
    /// The follow-up data (message + variant)
    pub data: DraftFollowUpData,
    /// Timestamp when the message was queued
    pub queued_at: DateTime<Utc>,
}

/// Status of the queue for a session (for frontend display)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum QueueStatus {
    /// No message queued
    Empty,
    /// Message is queued and waiting for execution to complete
    Queued { message: QueuedMessage },
}

/// The legacy editor has one replaceable draft per session. Once promoted,
/// each draft is an independent entry in the global execution queue.
#[derive(Clone)]
pub struct QueuedMessageService {
    pool: SqlitePool,
    pub scheduler_changed: Arc<Notify>,
}

impl QueuedMessageService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            scheduler_changed: Arc::new(Notify::new()),
        }
    }

    pub async fn queue_message(
        &self,
        session_id: Uuid,
        data: DraftFollowUpData,
    ) -> Result<QueuedMessage, Error> {
        Ok(StoredFollowUp::save(&self.pool, session_id, data)
            .await?
            .into())
    }

    pub async fn cancel_queued(&self, session_id: Uuid) -> Result<(), Error> {
        StoredFollowUp::delete(&self.pool, session_id, None).await?;
        Ok(())
    }

    pub async fn has_queued(&self, session_id: Uuid) -> Result<bool, Error> {
        Ok(StoredFollowUp::get(&self.pool, session_id).await?.is_some())
    }

    pub async fn get_status(&self, session_id: Uuid) -> Result<QueueStatus, Error> {
        Ok(match StoredFollowUp::get(&self.pool, session_id).await? {
            Some(message) => QueueStatus::Queued {
                message: message.into(),
            },
            None => QueueStatus::Empty,
        })
    }
}

impl From<StoredFollowUp> for QueuedMessage {
    fn from(stored: StoredFollowUp) -> Self {
        Self {
            session_id: stored.session_id,
            data: stored.payload.0,
            queued_at: stored.queued_at,
        }
    }
}
