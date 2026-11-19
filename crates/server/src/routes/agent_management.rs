use std::collections::HashSet;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, patch},
};
use db::models::{
    execution_queue::{ExecutionSchedulerConfig, ScheduledExecution},
    execution_usage::{ExecutionPrice, ExecutionUsageQuery, ExecutionUsageTotal},
};
use deployment::Deployment;
use serde::Deserialize;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/execution-usage", get(usage))
        .route("/execution-usage/prices", get(prices).put(update_prices))
        .route("/execution-queue", get(queue))
        .route("/execution-queue/config", get(config).put(update_config))
        .route("/execution-queue/{id}", patch(set_priority))
}

async fn usage(
    State(deployment): State<DeploymentImpl>,
    Query(filters): Query<ExecutionUsageQuery>,
) -> Result<Json<ApiResponse<Vec<ExecutionUsageTotal>>>, ApiError> {
    if let (Some(from), Some(to)) = (filters.from, filters.to)
        && from >= to
    {
        return Err(ApiError::BadRequest("from must precede to".into()));
    }
    Ok(Json(ApiResponse::success(
        ExecutionUsageTotal::aggregate(&deployment.db().pool, filters).await?,
    )))
}

async fn prices(
    State(deployment): State<DeploymentImpl>,
) -> Result<Json<ApiResponse<Vec<ExecutionPrice>>>, ApiError> {
    Ok(Json(ApiResponse::success(
        ExecutionPrice::list(&deployment.db().pool).await?,
    )))
}

async fn update_prices(
    State(deployment): State<DeploymentImpl>,
    Json(prices): Json<Vec<ExecutionPrice>>,
) -> Result<Json<ApiResponse<Vec<ExecutionPrice>>>, ApiError> {
    let mut keys = HashSet::with_capacity(prices.len());
    for price in &prices {
        if price.executor.trim().is_empty()
            || price.model.trim().is_empty()
            || !keys.insert((&price.executor, &price.model))
            || [
                price.input_usd_per_million,
                price.output_usd_per_million,
                price.cache_read_usd_per_million,
                price.cache_write_usd_per_million,
            ]
            .into_iter()
            .any(|rate| !rate.is_finite() || rate < 0.0)
        {
            return Err(ApiError::BadRequest(
                "Prices require distinct executor/model pairs and finite nonnegative rates".into(),
            ));
        }
    }
    ExecutionPrice::replace_all(&deployment.db().pool, &prices).await?;
    Ok(Json(ApiResponse::success(prices)))
}

async fn queue(
    State(deployment): State<DeploymentImpl>,
) -> Result<Json<ApiResponse<Vec<ScheduledExecution>>>, ApiError> {
    Ok(Json(ApiResponse::success(
        ScheduledExecution::list(&deployment.db().pool).await?,
    )))
}

async fn config(
    State(deployment): State<DeploymentImpl>,
) -> Result<Json<ApiResponse<ExecutionSchedulerConfig>>, ApiError> {
    Ok(Json(ApiResponse::success(
        ExecutionSchedulerConfig::get(&deployment.db().pool).await?,
    )))
}

async fn update_config(
    State(deployment): State<DeploymentImpl>,
    Json(config): Json<ExecutionSchedulerConfig>,
) -> Result<Json<ApiResponse<ExecutionSchedulerConfig>>, ApiError> {
    if config.max_concurrent == 0 {
        return Err(ApiError::BadRequest(
            "max_concurrent must be at least 1".into(),
        ));
    }
    config.save(&deployment.db().pool).await?;
    deployment
        .queued_message_service()
        .scheduler_changed
        .notify_one();
    Ok(Json(ApiResponse::success(config)))
}

#[derive(Debug, Deserialize, TS)]
pub struct ExecutionQueuePriority {
    pub priority: i32,
}

async fn set_priority(
    State(deployment): State<DeploymentImpl>,
    Path(id): Path<Uuid>,
    Json(payload): Json<ExecutionQueuePriority>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    if !ScheduledExecution::set_priority(&deployment.db().pool, id, payload.priority).await? {
        return Err(ApiError::Conflict("Execution is no longer queued".into()));
    }
    deployment
        .queued_message_service()
        .scheduler_changed
        .notify_one();
    Ok(Json(ApiResponse::success(())))
}
