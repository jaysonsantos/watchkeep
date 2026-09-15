//! `/api/sync`: run a Plex library sync.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use eyre::eyre;

use super::SyncResponse;
use crate::app::SharedContext;
use crate::http::{ApiResult, bad_request};

pub fn routes() -> Router<SharedContext> {
    Router::new().route("/sync", post(sync))
}

/// Waits for the sync and returns its report. Concurrent calls share one run.
async fn sync(State(ctx): State<SharedContext>) -> ApiResult {
    if !ctx.config.sync_configured() {
        return Ok(bad_request(
            "Plex sync is not configured. Set WATCHKEEP_PLEX_URL and WATCHKEEP_PLEX_TOKEN.",
        ));
    }
    let report = ctx.sync().await.map_err(|error| eyre!("{error}"))?;
    Ok(Json(SyncResponse { ok: true, report }).into_response())
}
