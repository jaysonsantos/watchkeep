//! `/api/episodes`: the watched toggle of a known episode.

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::routing::post;
use uuid::Uuid;
use watchkeep_storage::model::TargetKind;

use super::set_watched;
use crate::app::SharedContext;
use crate::http::ApiResult;

pub fn routes() -> Router<SharedContext> {
    Router::new().route("/episodes/{id}/watched", post(watch).delete(unwatch))
}

async fn watch(State(ctx): State<SharedContext>, Path(id): Path<Uuid>, body: Bytes) -> ApiResult {
    set_watched(&ctx, TargetKind::Episode, id, Some(&body), true).await
}

async fn unwatch(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    set_watched(&ctx, TargetKind::Episode, id, None, false).await
}
