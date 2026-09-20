//! `/api/recommendations`: what to watch next, from the watch history or from
//! the user ratings.

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};

use super::RecommendationsQuery;
use crate::app::SharedContext;
use crate::http::ApiResult;

pub fn routes() -> Router<SharedContext> {
    Router::new().route("/recommendations", get(list))
}

/// Without a catalog every list is empty, so the page shows its own notice.
async fn list(
    State(ctx): State<SharedContext>,
    Query(query): Query<RecommendationsQuery>,
) -> ApiResult {
    Ok(Json(ctx.recommender.recommendations(query.input).await?).into_response())
}
