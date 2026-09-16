//! Axum routes for the JSON API and the Plex webhook, plus the error type and
//! the response bodies they share.

pub mod api;
pub mod webhook;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use watchkeep_telemetry::report_error;

/// Response headers that Watchkeep sets.
pub mod header {
    /// The number of matches before `limit` and `offset`, on list routes.
    pub const TOTAL_COUNT: &str = "x-total-count";
    /// An alternative to `?token=` on the webhook URL.
    pub const WEBHOOK_TOKEN: &str = "x-webhook-token";
}

/// Any failure inside a handler. Reports the error on the span of the request
/// and answers `500 {"error": ...}`.
pub struct AppError(pub eyre::Report);

impl<E: Into<eyre::Report>> From<E> for AppError {
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        report_error!("the handler failed", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}

pub type ApiResult<T = Response> = Result<T, AppError>;

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct OkResponse {
    pub ok: bool,
}

pub fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: "not found".to_owned(),
        }),
    )
        .into_response()
}

pub fn bad_request(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: message.to_owned(),
        }),
    )
        .into_response()
}

pub fn ok() -> Response {
    Json(OkResponse { ok: true }).into_response()
}
