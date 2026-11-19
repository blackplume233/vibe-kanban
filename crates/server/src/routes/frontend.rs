use std::path::Path;

use axum::{Router, http::StatusCode};
use tower_http::services::ServeDir;

pub(super) fn with_api(api: Router, web_root: Option<&Path>) -> Router {
    // Keep missing API routes from inheriting the static fallback.
    let mut app = Router::new().nest("/api", api.fallback(|| async { StatusCode::NOT_FOUND }));
    if let Some(root) = web_root {
        app = app.fallback_service(ServeDir::new(root).precompressed_br().precompressed_gzip());
    }
    app
}
