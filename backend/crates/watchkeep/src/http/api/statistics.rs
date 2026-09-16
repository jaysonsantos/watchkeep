//! `/api/statistics`: the aggregates of the play history.

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};

use super::StatisticsQuery;
use crate::app::SharedContext;
use crate::http::ApiResult;

pub fn routes() -> Router<SharedContext> {
    Router::new().route("/statistics", get(statistics))
}

async fn statistics(
    State(ctx): State<SharedContext>,
    Query(query): Query<StatisticsQuery>,
) -> ApiResult {
    let now = ctx.clock.now();
    Ok(Json(ctx.queries.statistics(&query.tz, now).await?).into_response())
}
