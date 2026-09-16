//! The webhook routes: `POST /webhook/plex` and `POST /webhook/scrobble`.
//! Both check `WATCHKEEP_WEBHOOK_TOKEN`, and the origin check in `csrf.rs`
//! skips the whole prefix, because Plex posts forms without an Origin header.

pub mod plex;
pub mod scrobble;

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::{ErrorResponse, header};
use crate::app::SharedContext;
use crate::scrobble::ScrobbleResult;

/// Longer payloads are cut before they go into the webhook log.
const MAX_LOGGED_PAYLOAD: usize = 64 * 1024;

/// The outcome label of an event that Watchkeep does not track.
pub(super) const OUTCOME_IGNORED: &str = "ignored";

/// The outcome label of an event without a valid token, without a payload, or
/// with a body that the endpoint refuses.
pub(super) const OUTCOME_REJECTED: &str = "rejected";

/// The event label of a call that carried no event name.
pub(super) const EVENT_NONE: &str = "none";

/// The outcome label of a call that ended in an internal error.
pub(super) const OUTCOME_FAILED: &str = "failed";

/// One point per webhook call, for both endpoints. The event name and the
/// outcome are words of an enum, so the cardinality stays small.
pub(super) fn count_event(event: Option<&str>, outcome: &str) {
    tracing::info!(
        monotonic_counter.watchkeep_webhook_events_total = 1_u64,
        webhook_event = event.unwrap_or(EVENT_NONE),
        webhook_outcome = outcome,
    );
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WebhookQuery {
    pub token: Option<String>,
}

/// The webhook answer for an event that Watchkeep does not track.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct IgnoredResponse {
    pub ok: bool,
    pub ignored: String,
}

/// The webhook answer for a tracked event.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct WebhookResponse {
    pub ok: bool,
    #[serde(flatten)]
    pub result: ScrobbleResult,
}

pub fn routes() -> Router<SharedContext> {
    Router::new()
        .route("/plex", post(plex::plex))
        .route("/scrobble", post(scrobble::scrobble))
}

/// `401` when the configured token is set and the caller sends another one.
/// The token comes from `?token=` or from the `x-webhook-token` header.
fn unauthorized(expected: &str, query: &WebhookQuery, headers: &HeaderMap) -> Option<Response> {
    if expected.is_empty() {
        return None;
    }
    let given = query
        .token
        .clone()
        .or_else(|| {
            headers
                .get(header::WEBHOOK_TOKEN)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
        .unwrap_or_default();
    if given == expected {
        return None;
    }
    Some(
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid token".to_owned(),
            }),
        )
            .into_response(),
    )
}

/// The first `MAX_LOGGED_PAYLOAD` bytes, cut at a character boundary.
fn truncate(text: &str) -> String {
    if text.len() <= MAX_LOGGED_PAYLOAD {
        return text.to_owned();
    }
    let mut end = MAX_LOGGED_PAYLOAD;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}
