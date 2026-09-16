//! `/api/recommendations`: what to watch next, from the watch history.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};

use crate::app::SharedContext;
use crate::http::ApiResult;

pub fn routes() -> Router<SharedContext> {
    Router::new().route("/recommendations", get(list))
}

/// Without a catalog every list is empty, so the page shows its own notice.
async fn list(State(ctx): State<SharedContext>) -> ApiResult {
    Ok(Json(ctx.recommender.recommendations().await?).into_response())
}
