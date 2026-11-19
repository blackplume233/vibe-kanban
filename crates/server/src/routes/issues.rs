use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};
use db::models::{
    issue::{
        CreateIssue, Issue, IssueBatchUpdate, IssueError, IssueFilter, UpdateIssue, require_project,
    },
    issue_board::{
        IssueCycle, IssueSavedView, IssueStatus, IssueTag, ReorderIssueStatuses, SaveIssueCycle,
        SaveIssueStatus, SaveIssueTag, SaveIssueView,
    },
    issue_links::{
        CreateIssueComment, CreateIssueRelationship, IssueComment, IssueRelationship,
        UpdateIssueComment,
    },
    issue_sync::IssueSyncStoreError,
    project::{CreateLocalProject, Project},
};
use deployment::Deployment;
use services::services::github_issue_sync::{
    IssueSyncError, SyncGitHubIssueRequest, SyncGitHubIssueResponse, sync_github_issue,
};
use sqlx::{SqlitePool, error::ErrorKind};
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

enum IssueApiError {
    NotFound,
    Api(ApiError),
}

impl IntoResponse for IssueApiError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<()>::error(
                    "Local project or resource not found.",
                )),
            )
                .into_response(),
            Self::Api(error) => error.into_response(),
        }
    }
}

impl From<sqlx::Error> for IssueApiError {
    fn from(error: sqlx::Error) -> Self {
        match &error {
            sqlx::Error::RowNotFound => Self::NotFound,
            sqlx::Error::Database(source) => Self::Api(match source.kind() {
                ErrorKind::UniqueViolation => {
                    ApiError::Conflict("This value already exists in the project.".into())
                }
                ErrorKind::ForeignKeyViolation => {
                    ApiError::BadRequest("Referenced resources must exist in this project.".into())
                }
                ErrorKind::CheckViolation => ApiError::BadRequest(
                    "The supplied resource fields violate a constraint.".into(),
                ),
                _ if source.code().as_deref() == Some("1811") => {
                    ApiError::BadRequest(source.message().into())
                }
                _ => ApiError::Database(error),
            }),
            _ => Self::Api(ApiError::Database(error)),
        }
    }
}

impl From<IssueError> for IssueApiError {
    fn from(error: IssueError) -> Self {
        match error {
            IssueError::Invalid(message) => Self::Api(ApiError::BadRequest(message.into())),
            IssueError::Database(error) => error.into(),
        }
    }
}

impl From<IssueSyncError> for IssueApiError {
    fn from(error: IssueSyncError) -> Self {
        match error {
            IssueSyncError::Invalid(message) => Self::Api(ApiError::BadRequest(message.into())),
            IssueSyncError::GitHub(error) => Self::Api(ApiError::GitHost(error)),
            IssueSyncError::Database(error)
            | IssueSyncError::Store(IssueSyncStoreError::Database(error)) => error.into(),
            IssueSyncError::Store(error) => Self::Api(ApiError::Conflict(error.to_string())),
        }
    }
}

type Reply<T> = Result<Json<ApiResponse<T>>, IssueApiError>;

fn reply<T>(result: Result<T, impl Into<IssueApiError>>) -> Reply<T> {
    result
        .map(|data| Json(ApiResponse::success(data)))
        .map_err(Into::into)
}

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/projects", get(list_projects).post(create_project))
        .route(
            "/projects/{project_id}/issues",
            get(list_issues).post(create_issue),
        )
        .route("/projects/{project_id}/issues/batch", patch(update_batch))
        .route(
            "/projects/{project_id}/issues/{issue_id}",
            get(get_issue).patch(update_issue).delete(delete_issue),
        )
        .route(
            "/projects/{project_id}/issues/{issue_id}/children",
            get(list_children).post(create_child),
        )
        .route(
            "/projects/{project_id}/issues/{issue_id}/relationships",
            get(list_relationships).post(create_relationship),
        )
        .route(
            "/projects/{project_id}/issues/{issue_id}/relationships/{id}",
            delete(delete_relationship),
        )
        .route(
            "/projects/{project_id}/issues/{issue_id}/comments",
            get(list_comments).post(create_comment),
        )
        .route(
            "/projects/{project_id}/issues/{issue_id}/comments/{id}",
            put(update_comment).delete(delete_comment),
        )
        .route(
            "/projects/{project_id}/statuses/reorder",
            put(reorder_statuses),
        )
        .merge(statuses::router())
        .merge(tags::router())
        .merge(cycles::router())
        .merge(views::router())
        // vibe-own M5
        .route(
            "/projects/{project_id}/github/issues/sync",
            post(sync_issue),
        )
}

async fn sync_issue(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
    Json(request): Json<SyncGitHubIssueRequest>,
) -> Reply<SyncGitHubIssueResponse> {
    reply(sync_github_issue(&deployment.db().pool, project_id, &request).await)
}

async fn list_projects(State(deployment): State<DeploymentImpl>) -> Reply<Vec<Project>> {
    reply(Project::find_all(&deployment.db().pool).await)
}

async fn create_project(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateLocalProject>,
) -> Reply<Project> {
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(IssueApiError::Api(ApiError::BadRequest(
            "Project name is required.".into(),
        )));
    }
    reply(Project::create(&deployment.db().pool, name).await)
}

async fn list_issues(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
    Query(filter): Query<IssueFilter>,
) -> Reply<Vec<Issue>> {
    let pool = &deployment.db().pool;
    require_project(pool, project_id).await?;
    reply(Issue::list(pool, project_id, &filter).await)
}

async fn create_issue(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<CreateIssue>,
) -> Reply<Issue> {
    let pool = &deployment.db().pool;
    require_project(pool, project_id).await?;
    reply(Issue::create(pool, project_id, &payload).await)
}

async fn get_issue(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, issue_id)): Path<(Uuid, Uuid)>,
) -> Reply<Issue> {
    reply(Issue::get(&deployment.db().pool, project_id, issue_id).await)
}

async fn update_issue(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, issue_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateIssue>,
) -> Reply<Issue> {
    reply(Issue::update(&deployment.db().pool, project_id, issue_id, &payload).await)
}

async fn update_batch(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
    Json(updates): Json<Vec<IssueBatchUpdate>>,
) -> Reply<Vec<Issue>> {
    let pool = &deployment.db().pool;
    require_project(pool, project_id).await?;
    reply(Issue::update_batch(pool, project_id, &updates).await)
}

async fn delete_issue(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, issue_id)): Path<(Uuid, Uuid)>,
) -> Reply<()> {
    reply(Issue::delete(&deployment.db().pool, project_id, issue_id).await)
}

async fn require_issue(
    pool: &SqlitePool,
    project_id: Uuid,
    issue_id: Uuid,
) -> Result<(), IssueApiError> {
    Issue::exists(pool, project_id, issue_id)
        .await?
        .then_some(())
        .ok_or(IssueApiError::NotFound)
}

async fn list_children(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, issue_id)): Path<(Uuid, Uuid)>,
) -> Reply<Vec<Issue>> {
    let pool = &deployment.db().pool;
    require_issue(pool, project_id, issue_id).await?;
    let filter = IssueFilter {
        parent_issue_id: Some(issue_id),
        ..Default::default()
    };
    reply(Issue::list(pool, project_id, &filter).await)
}

async fn create_child(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, issue_id)): Path<(Uuid, Uuid)>,
    Json(mut payload): Json<CreateIssue>,
) -> Reply<Issue> {
    let pool = &deployment.db().pool;
    require_issue(pool, project_id, issue_id).await?;
    if payload
        .parent_issue_id
        .is_some_and(|parent| parent != issue_id)
    {
        return Err(IssueApiError::Api(ApiError::BadRequest(
            "parent_issue_id must match the URL.".into(),
        )));
    }
    payload.parent_issue_id = Some(issue_id);
    reply(Issue::create(pool, project_id, &payload).await)
}

async fn list_relationships(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, issue_id)): Path<(Uuid, Uuid)>,
) -> Reply<Vec<IssueRelationship>> {
    let pool = &deployment.db().pool;
    require_issue(pool, project_id, issue_id).await?;
    reply(IssueRelationship::list(pool, project_id, issue_id).await)
}

async fn create_relationship(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, issue_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<CreateIssueRelationship>,
) -> Reply<IssueRelationship> {
    let pool = &deployment.db().pool;
    require_issue(pool, project_id, issue_id).await?;
    reply(IssueRelationship::create(pool, project_id, issue_id, &payload).await)
}

async fn delete_relationship(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, issue_id, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Reply<()> {
    reply(IssueRelationship::delete(&deployment.db().pool, project_id, issue_id, id).await)
}

async fn list_comments(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, issue_id)): Path<(Uuid, Uuid)>,
) -> Reply<Vec<IssueComment>> {
    let pool = &deployment.db().pool;
    require_issue(pool, project_id, issue_id).await?;
    reply(IssueComment::list(pool, project_id, issue_id).await)
}

async fn create_comment(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, issue_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<CreateIssueComment>,
) -> Reply<IssueComment> {
    let pool = &deployment.db().pool;
    require_issue(pool, project_id, issue_id).await?;
    reply(IssueComment::create(pool, project_id, issue_id, &payload).await)
}

async fn update_comment(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, issue_id, id)): Path<(Uuid, Uuid, Uuid)>,
    Json(payload): Json<UpdateIssueComment>,
) -> Reply<IssueComment> {
    reply(IssueComment::update(&deployment.db().pool, project_id, issue_id, id, &payload).await)
}

async fn delete_comment(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, issue_id, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Reply<()> {
    reply(IssueComment::delete(&deployment.db().pool, project_id, issue_id, id).await)
}

async fn reorder_statuses(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<ReorderIssueStatuses>,
) -> Reply<Vec<IssueStatus>> {
    reply(IssueStatus::reorder(&deployment.db().pool, project_id, &payload.status_ids).await)
}

macro_rules! board_resource {
    ($module:ident, $path:literal, $model:ty, $payload:ty) => {
        mod $module {
            use super::*;

            pub(super) fn router() -> Router<DeploymentImpl> {
                Router::new()
                    .route(
                        concat!("/projects/{project_id}/", $path),
                        get(list).post(create),
                    )
                    .route(
                        concat!("/projects/{project_id}/", $path, "/{id}"),
                        get(read).put(update).delete(remove),
                    )
            }

            async fn list(
                State(deployment): State<DeploymentImpl>,
                Path(project_id): Path<Uuid>,
            ) -> Reply<Vec<$model>> {
                let pool = &deployment.db().pool;
                require_project(pool, project_id).await?;
                reply(<$model>::list(pool, project_id).await)
            }

            async fn create(
                State(deployment): State<DeploymentImpl>,
                Path(project_id): Path<Uuid>,
                Json(payload): Json<$payload>,
            ) -> Reply<$model> {
                let pool = &deployment.db().pool;
                require_project(pool, project_id).await?;
                reply(<$model>::create(pool, project_id, &payload).await)
            }

            async fn read(
                State(deployment): State<DeploymentImpl>,
                Path((project_id, id)): Path<(Uuid, Uuid)>,
            ) -> Reply<$model> {
                reply(<$model>::get(&deployment.db().pool, project_id, id).await)
            }

            async fn update(
                State(deployment): State<DeploymentImpl>,
                Path((project_id, id)): Path<(Uuid, Uuid)>,
                Json(payload): Json<$payload>,
            ) -> Reply<$model> {
                reply(<$model>::update(&deployment.db().pool, project_id, id, &payload).await)
            }

            async fn remove(
                State(deployment): State<DeploymentImpl>,
                Path((project_id, id)): Path<(Uuid, Uuid)>,
            ) -> Reply<()> {
                reply(<$model>::delete(&deployment.db().pool, project_id, id).await)
            }
        }
    };
}

board_resource!(statuses, "statuses", IssueStatus, SaveIssueStatus);
board_resource!(tags, "tags", IssueTag, SaveIssueTag);
board_resource!(cycles, "cycles", IssueCycle, SaveIssueCycle);
board_resource!(views, "saved-views", IssueSavedView, SaveIssueView);
