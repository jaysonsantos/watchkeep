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
        return Ok(bad_request("missing payload"));
    };
    let Ok(raw) = serde_json::from_str::<Value>(&text) else {
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
            return Ok(Json(IgnoredResponse {
                ok: true,
                ignored: reason,
            })
            .into_response());
        }
        ParseResult::Event(event) => event,
    };

    let result = ctx.scrobbler.apply(&event).await?;
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
