//! `POST /webhook/plex`: the Plex webhook.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{FromRequest, Multipart, Query, Request, State};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Form, Json, Router};
use eyre::{Result, eyre};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{Span, field, instrument};
use watchkeep_storage::library::WebhookLog;

use super::{ApiResult, ErrorResponse, bad_request, header};
use crate::app::SharedContext;
use crate::plex::payload::{ParseResult, parse_plex_payload};
use crate::scrobble::ScrobbleResult;

/// The form field that holds the JSON payload.
const PAYLOAD_FIELD: &str = "payload";

/// Longer payloads are cut before they go into the webhook log.
const MAX_LOGGED_PAYLOAD: usize = 64 * 1024;

const MULTIPART: &str = "multipart/form-data";
const FORM_URLENCODED: &str = "application/x-www-form-urlencoded";

/// The outcome label of an event that Watchkeep does not track.
const OUTCOME_IGNORED: &str = "ignored";

/// The outcome label of an event without a valid token or without a payload.
const OUTCOME_REJECTED: &str = "rejected";

/// The event label of a call that carried no Plex event name.
const EVENT_NONE: &str = "none";

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
    Router::new().route("/plex", post(plex))
}

/// Plex sends `multipart/form-data` with a `payload` field that holds JSON.
/// Some proxies forward plain JSON. Both shapes are accepted.
async fn read_payload(request: Request) -> Result<Option<String>> {
    let content_type = request
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if content_type.contains(MULTIPART) {
        let mut multipart = Multipart::from_request(request, &())
            .await
            .map_err(|error| eyre!("{error}"))?;
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|error| eyre!("{error}"))?
        {
            if field.name() == Some(PAYLOAD_FIELD) {
                return Ok(Some(field.text().await.map_err(|error| eyre!("{error}"))?));
            }
        }
        return Ok(None);
    }
    if content_type.contains(FORM_URLENCODED) {
        let Form(form) = Form::<HashMap<String, String>>::from_request(request, &())
            .await
            .map_err(|error| eyre!("{error}"))?;
        return Ok(form.get(PAYLOAD_FIELD).cloned());
    }
    let bytes = Bytes::from_request(request, &())
        .await
        .map_err(|error| eyre!("{error}"))?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok(if text.trim().is_empty() {
        None
    } else {
        Some(text)
    })
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

/// The webhook is the entry point of the scrobbler. The span names the event
/// and the outcome, and one counter counts the events by both.
#[instrument(
    skip_all,
    // The account of the event is the name of a person, so it stays off the
    // span. The webhook log keeps it for the UI.
    fields(plex.event = field::Empty, webhook.outcome = field::Empty)
)]
async fn plex(
    State(ctx): State<SharedContext>,
    Query(query): Query<WebhookQuery>,
    request: Request,
) -> ApiResult {
    let expected = &ctx.config.webhook_token;
    if !expected.is_empty() {
        let given = query
            .token
            .or_else(|| {
                request
                    .headers()
                    .get(header::WEBHOOK_TOKEN)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        if given != *expected {
            count_event(None, OUTCOME_REJECTED);
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "invalid token".to_owned(),
                }),
            )
                .into_response());
        }
    }

    let Some(text) = read_payload(request).await? else {
        count_event(None, OUTCOME_REJECTED);
        return Ok(bad_request("missing payload"));
    };
    let Ok(raw) = serde_json::from_str::<Value>(&text) else {
        count_event(None, OUTCOME_REJECTED);
        return Ok(bad_request("payload is not JSON"));
    };

    let logged = truncate(&text);
    let mut library = ctx.library().await?;
    let event = match parse_plex_payload(&raw) {
        ParseResult::Ignored {
            reason,
            event,
            media_type,
            title,
        } => {
            library
                .log_webhook(WebhookLog {
                    event,
                    account: None,
                    player: None,
                    media_type,
                    title,
                    outcome: format!("ignored: {reason}"),
                    payload: logged,
                })
                .await?;
            count_event(None, OUTCOME_IGNORED);
            return Ok(Json(IgnoredResponse {
                ok: true,
                ignored: reason,
            })
            .into_response());
        }
        ParseResult::Event(event) => event,
    };

    let span = Span::current();
    span.record("plex.event", event.event.as_str());
    let result = ctx.scrobbler.apply(&event).await?;
    span.record("webhook.outcome", result.action.as_str());
    library
        .log_webhook(WebhookLog {
            event: Some(event.event.to_string()),
            account: event.account.clone(),
            player: event.player.clone(),
            media_type: Some(event.media.target_kind().to_string()),
            title: Some(result.title.clone()),
            outcome: result.action.to_string(),
            payload: logged,
        })
        .await?;
    library
        .prune_webhook_log(ctx.config.webhook_retention)
        .await?;
    count_event(Some(event.event.as_str()), result.action.as_str());
    tracing::info!(
        "{} {} \"{}\"{}",
        event.event,
        result.action,
        result.title,
        result
            .percent
            .map(|percent| format!(" {percent}%"))
            .unwrap_or_default()
    );
    Ok(Json(WebhookResponse { ok: true, result }).into_response())
}

/// One point per webhook call. The event name and the outcome are words of an
/// enum, so the cardinality stays small.
fn count_event(event: Option<&str>, outcome: &str) {
    tracing::info!(
        monotonic_counter.watchkeep_webhook_events_total = 1_u64,
        plex_event = event.unwrap_or(EVENT_NONE),
        webhook_outcome = outcome,
    );
}
