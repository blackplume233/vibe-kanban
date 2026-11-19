use chrono::{DateTime, Utc};
use executors::logs::ExecutionTokenUsage;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Deserialize, TS)]
pub struct ExecutionUsageQuery {
    pub session_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, TS, FromRow)]
pub struct ExecutionUsageTotal {
    pub executor: String,
    pub model: Option<String>,
    #[ts(type = "number")]
    pub input_tokens: i64,
    #[ts(type = "number")]
    pub output_tokens: i64,
    #[ts(type = "number")]
    pub cache_read_tokens: i64,
    #[ts(type = "number")]
    pub cache_write_tokens: i64,
    pub cost_usd: Option<f64>,
    #[ts(type = "number")]
    pub unpriced_samples: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, FromRow)]
pub struct ExecutionPrice {
    pub executor: String,
    /// Exact model ID, or `*` for the executor's default rates.
    pub model: String,
    pub input_usd_per_million: f64,
    pub output_usd_per_million: f64,
    pub cache_read_usd_per_million: f64,
    pub cache_write_usd_per_million: f64,
}

impl ExecutionPrice {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM execution_prices ORDER BY executor, model")
            .fetch_all(pool)
            .await
    }

    pub async fn replace_all(pool: &SqlitePool, prices: &[Self]) -> Result<(), sqlx::Error> {
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM execution_prices")
            .execute(&mut *tx)
            .await?;
        for price in prices {
            sqlx::query("INSERT INTO execution_prices VALUES (?, ?, ?, ?, ?, ?)")
                .bind(&price.executor)
                .bind(&price.model)
                .bind(price.input_usd_per_million)
                .bind(price.output_usd_per_million)
                .bind(price.cache_read_usd_per_million)
                .bind(price.cache_write_usd_per_million)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await
    }
}

pub struct ExecutionUsage<'a> {
    pub execution_process_id: Uuid,
    pub session_id: Uuid,
    pub executor: &'a str,
    pub model: Option<&'a str>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost_usd: Option<f64>,
}

impl ExecutionUsage<'_> {
    pub async fn record(self, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        if !self.has_usage() {
            return Ok(());
        }
        self.insert(pool).await
    }

    pub async fn record_snapshot(
        mut self,
        pool: &SqlitePool,
        agent_session_id: &str,
        snapshot: &ExecutionTokenUsage,
    ) -> Result<(), sqlx::Error> {
        let mut tx = pool.begin().await?;
        let model_key = snapshot.model.as_deref().unwrap_or_default();
        // Acquire the write lock before reading the previous totals.
        sqlx::query("INSERT OR IGNORE INTO execution_usage_baselines VALUES (?, ?, ?, ?, ?)")
            .bind(self.session_id)
            .bind(self.executor)
            .bind(agent_session_id)
            .bind(model_key)
            .bind(sqlx::types::Json(ExecutionTokenUsage::default()))
            .execute(&mut *tx)
            .await?;
        let previous: sqlx::types::Json<ExecutionTokenUsage> = sqlx::query_scalar(
            "SELECT counters FROM execution_usage_baselines WHERE session_id = ? AND executor = ? AND agent_session_id = ? AND model = ?",
        ).bind(self.session_id).bind(self.executor).bind(agent_session_id).bind(model_key).fetch_one(&mut *tx).await?;
        let mut previous = previous.0;
        let delta = |latest: u64, prior: &mut u64| {
            let delta = latest.saturating_sub(*prior);
            *prior = (*prior).max(latest);
            i64::try_from(delta).unwrap_or(i64::MAX)
        };
        self.input_tokens = delta(snapshot.input_tokens, &mut previous.input_tokens);
        self.output_tokens = delta(snapshot.output_tokens, &mut previous.output_tokens);
        self.cache_read_tokens = delta(snapshot.cache_read_tokens, &mut previous.cache_read_tokens);
        self.cache_write_tokens = delta(
            snapshot.cache_write_tokens,
            &mut previous.cache_write_tokens,
        );
        self.cost_usd = snapshot.cost_usd.map(|cost| {
            let prior = previous.cost_usd.unwrap_or_default();
            previous.cost_usd = Some(cost.max(prior));
            (cost - prior).max(0.0)
        });
        sqlx::query("UPDATE execution_usage_baselines SET counters = ? WHERE session_id = ? AND executor = ? AND agent_session_id = ? AND model = ?")
            .bind(sqlx::types::Json(previous)).bind(self.session_id).bind(self.executor).bind(agent_session_id).bind(model_key)
            .execute(&mut *tx).await?;
        if self.has_usage() {
            self.insert(&mut *tx).await?;
        }
        tx.commit().await
    }

    fn has_usage(&self) -> bool {
        self.input_tokens > 0
            || self.output_tokens > 0
            || self.cache_read_tokens > 0
            || self.cache_write_tokens > 0
            || self.cost_usd.is_some_and(|cost| cost > 0.0)
    }

    async fn insert<'e>(
        self,
        db: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO execution_usage (execution_process_id, session_id, executor, model,
                 input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd, recorded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, COALESCE(?9, (
                 SELECT (?5 * input_usd_per_million + ?6 * output_usd_per_million
                        + ?7 * cache_read_usd_per_million + ?8 * cache_write_usd_per_million) / 1000000.0
                 FROM execution_prices WHERE executor = ?3 AND model IN (?4, '*')
                 ORDER BY model = ?4 DESC LIMIT 1
             )), ?10)",
        )
        .bind(self.execution_process_id)
        .bind(self.session_id)
        .bind(self.executor)
        .bind(self.model)
        .bind(self.input_tokens)
        .bind(self.output_tokens)
        .bind(self.cache_read_tokens)
        .bind(self.cache_write_tokens)
        .bind(self.cost_usd)
        .bind(Utc::now())
        .execute(db)
        .await?;
        Ok(())
    }
}

impl ExecutionUsageTotal {
    pub async fn aggregate(
        pool: &SqlitePool,
        filters: ExecutionUsageQuery,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let mut sql = sqlx::QueryBuilder::new(
            "SELECT u.executor, u.model, SUM(u.input_tokens) AS input_tokens,
                SUM(u.output_tokens) AS output_tokens, SUM(u.cache_read_tokens) AS cache_read_tokens,
                SUM(u.cache_write_tokens) AS cache_write_tokens, SUM(u.cost_usd) AS cost_usd,
                SUM(u.cost_usd IS NULL) AS unpriced_samples
             FROM execution_usage u JOIN sessions s ON s.id = u.session_id WHERE 1 = 1",
        );
        if let Some(session_id) = filters.session_id {
            sql.push(" AND u.session_id = ").push_bind(session_id);
        }
        if let Some(workspace_id) = filters.workspace_id {
            sql.push(" AND s.workspace_id = ").push_bind(workspace_id);
        }
        if let Some(from) = filters.from {
            sql.push(" AND u.recorded_at >= ").push_bind(from);
        }
        if let Some(to) = filters.to {
            sql.push(" AND u.recorded_at < ").push_bind(to);
        }
        sql.push(" GROUP BY u.executor, u.model ORDER BY u.executor, u.model");
        sql.build_query_as().fetch_all(pool).await
    }
}
