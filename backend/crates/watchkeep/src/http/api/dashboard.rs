//! Server config, counts, playback positions, and the webhook log.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get};
use axum::{Json, Router};
use uuid::Uuid;
use watchkeep_storage::model::TargetKind;

use super::{ServerConfig, WEBHOOK_LOG_LIMIT};
use crate::app::SharedContext;
use crate::http::{ApiResult, ok};

pub fn routes() -> Router<SharedContext> {
    Router::new()
        .route("/config", get(config))
        .route("/stats", get(stats))
        .route("/progress", get(progress))
        .route("/progress/{kind}/{id}", delete(clear_progress))
        .route("/webhooks", get(webhooks))
}

async fn config(State(ctx): State<SharedContext>) -> Json<ServerConfig> {
    Json(ServerConfig {
        image_base_url: ctx.config.image_base_url.clone(),
        sync_configured: ctx.config.sync_configured(),
        catalog_configured: ctx.catalog.is_some(),
    })
}

async fn stats(State(ctx): State<SharedContext>) -> ApiResult {
    Ok(Json(ctx.queries.stats().await?).into_response())
}

async fn progress(State(ctx): State<SharedContext>) -> ApiResult {
    Ok(Json(ctx.queries.in_progress().await?).into_response())
}

async fn clear_progress(
    State(ctx): State<SharedContext>,
    Path((kind, id)): Path<(TargetKind, Uuid)>,
) -> ApiResult {
    ctx.actions.clear_progress(kind, id).await?;
    Ok(ok())
}

async fn webhooks(State(ctx): State<SharedContext>) -> ApiResult {
    Ok(Json(ctx.queries.recent_webhooks(WEBHOOK_LOG_LIMIT).await?).into_response())
}
