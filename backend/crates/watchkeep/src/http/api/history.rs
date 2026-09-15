//! `/api/history`: plays, newest first, and removing one play.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get};
use axum::{Json, Router};
use uuid::Uuid;

use super::{DEFAULT_PAGE_LIMIT, HistoryPage, HistoryQuery};
use crate::app::SharedContext;
use crate::http::{ApiResult, not_found, ok};

pub fn routes() -> Router<SharedContext> {
    Router::new()
        .route("/history", get(list))
        .route("/history/{id}", delete(remove))
}

async fn list(State(ctx): State<SharedContext>, Query(query): Query<HistoryQuery>) -> ApiResult {
    let window = query.window();
    let limit = window.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
    Ok(Json(HistoryPage {
        total: ctx.queries.history_count().await?,
        items: ctx.queries.history(limit, window.offset).await?,
    })
    .into_response())
}

async fn remove(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    Ok(if ctx.actions.remove_play(id).await? {
        ok()
    } else {
        not_found()
    })
}
