//! `POST /webhook/scrobble`: one playback event from a player that is not Plex.
//!
//! The status code tells the sender what to do. `200` means the event is done,
//! `400` and `422` mean that a retry cannot help, and `5xx` asks for a retry
//! with the same `event_id`.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use eyre::Result;
use watchkeep_storage::library::{Library, WebhookLog};
use watchkeep_storage::model::MediaRef;

use super::{
    OUTCOME_FAILED, OUTCOME_REJECTED, WebhookQuery, WebhookResponse, count_event, truncate,
    unauthorized,
};
use crate::app::SharedContext;
use crate::http::{ApiResult, ErrorResponse, bad_request};
use crate::scrobble::event::{InvalidEvent, ScrobbleEvent};

const JSON: &str = "application/json";

/// The `outcome` of a webhook log row for a body that the endpoint refused.
const REFUSED: &str = "invalid";

/// `422`: the body is valid JSON, but no item can match it.
fn unprocessable(message: &str) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(ErrorResponse {
            error: message.to_owned(),
        }),
    )
        .into_response()
}

/// Write one row of the webhook log and drop the rows that aged out.
async fn log(ctx: &SharedContext, entry: WebhookLog) -> Result<()> {
    let mut library: Library<_> = ctx.library().await?;
    library.log_webhook(entry).await?;
    library
        .prune_webhook_log(ctx.config.webhook_retention)
        .await?;
    Ok(())
}

pub async fn scrobble(
    State(ctx): State<SharedContext>,
    Query(query): Query<WebhookQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult {
    let result = handle(ctx, query, headers, body).await;
    // A call that fails counts too, else the counter hides exactly the events
    // that Watchkeep lost.
    if result.is_err() {
        count_event(None, OUTCOME_FAILED);
    }
    result
}

/// The work of the endpoint. `scrobble` counts the outcome around it.
async fn handle(
    ctx: SharedContext,
    query: WebhookQuery,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult {
    if let Some(denied) = unauthorized(&ctx.config.webhook_token, &query, &headers) {
        count_event(None, OUTCOME_REJECTED);
        return Ok(denied);
    }
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !content_type.contains(JSON) {
        count_event(None, OUTCOME_REJECTED);
        return Ok(bad_request("the body must be application/json"));
    }
    let text = String::from_utf8_lossy(&body).into_owned();
    let event = match serde_json::from_str::<ScrobbleEvent>(&text) {
        Ok(event) => event,
        Err(error) => {
            count_event(None, OUTCOME_REJECTED);
            return Ok(bad_request(&error.to_string()));
        }
    };

    let logged = truncate(&text);
    let entry = |outcome: String, title: Option<String>| WebhookLog {
        event: Some(event.event.log_name()),
        account: event.viewer(),
        player: event.device(),
        media_type: Some(event.media.kind.to_string()),
        title,
        outcome,
        payload: logged.clone(),
    };

    let media: MediaRef = match event.media_ref() {
        Ok(media) => media,
        Err(error) => {
            log(&ctx, entry(format!("{REFUSED}: {error}"), None)).await?;
            count_event(Some(&event.event.log_name()), OUTCOME_REJECTED);
            return Ok(match error {
                InvalidEvent::Missing(_) => bad_request(&error.to_string()),
                InvalidEvent::Unidentified => unprocessable(&error.to_string()),
            });
        }
    };

    let result = ctx.scrobbler.apply_scrobble(&event, media).await?;
    log(
        &ctx,
        entry(result.action.to_string(), Some(result.title.clone())),
    )
    .await?;
    count_event(Some(&event.event.log_name()), result.action.as_str());
    tracing::info!(
        "{} {} \"{}\"{}",
        event.event.log_name(),
        result.action,
        result.title,
        result
            .percent
            .map(|percent| format!(" {percent}%"))
            .unwrap_or_default()
    );
    Ok(Json(WebhookResponse { ok: true, result }).into_response())
}
