//! `/api/watchlist`: the watchlist with watched counts.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};

use crate::app::SharedContext;
use crate::http::ApiResult;

pub fn routes() -> Router<SharedContext> {
    Router::new().route("/watchlist", get(list))
}

async fn list(State(ctx): State<SharedContext>) -> ApiResult {
    Ok(Json(ctx.queries.watchlist().await?).into_response())
}
