//! `POST /webhook/plex`: the Plex webhook.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{FromRequest, Multipart, Query, Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::{Form, Json};
use eyre::{Result, eyre};
use serde_json::Value;
use watchkeep_storage::library::WebhookLog;

use super::{IgnoredResponse, WebhookQuery, WebhookResponse, truncate, unauthorized};
use crate::app::SharedContext;
use crate::http::{ApiResult, bad_request};
use crate::plex::payload::{ParseResult, parse_plex_payload};

/// The form field that holds the JSON payload.
const PAYLOAD_FIELD: &str = "payload";

const MULTIPART: &str = "multipart/form-data";
const FORM_URLENCODED: &str = "application/x-www-form-urlencoded";

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

pub async fn plex(
    State(ctx): State<SharedContext>,
    Query(query): Query<WebhookQuery>,
    request: Request,
) -> ApiResult {
    if let Some(denied) = unauthorized(&ctx.config.webhook_token, &query, request.headers()) {
        return Ok(denied);
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
