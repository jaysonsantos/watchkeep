//! An origin check for form posts. Plex posts webhook forms without an Origin
//! header, so the check applies to the UI and the JSON API only, not to `/webhook`.
//!
//! Only the host is compared. Without `WATCHKEEP_HTTP_ORIGIN` the host comes
//! from the `Host` header, or from the header that `WATCHKEEP_HTTP_HOST_HEADER` names.

use axum::http::header::{CONTENT_TYPE, ORIGIN};
use axum::http::{HeaderMap, Method};

const FORM_TYPES: [&str; 3] = [
    "application/x-www-form-urlencoded",
    "multipart/form-data",
    "text/plain",
];

/// The `host[:port]` part of an origin string, the way `new URL(origin).host` reads it.
/// Returns `None` for values that are not a URL, for example `null`.
pub fn origin_host(origin: &str) -> Option<String> {
    let (scheme, rest) = origin.split_once("://")?;
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if host.is_empty() {
        return None;
    }
    let host = host.to_ascii_lowercase();
    let default_port = match scheme.to_ascii_lowercase().as_str() {
        "http" | "ws" => ":80",
        "https" | "wss" => ":443",
        _ => "",
    };
    Some(match host.strip_suffix(default_port) {
        Some(stripped) if !default_port.is_empty() => stripped.to_owned(),
        _ => host,
    })
}

/// The host that a same-site request must carry in its Origin header.
pub fn expected_host(http_origin: &str, host_header: &str, headers: &HeaderMap) -> String {
    if !http_origin.is_empty()
        && let Some(host) = origin_host(http_origin)
    {
        return host;
    }
    headers
        .get(host_header)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

/// True when the request is an unsafe form post from another site or from no site.
pub fn is_cross_site_form_post(method: &Method, headers: &HeaderMap, host: &str) -> bool {
    if !matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        return false;
    }
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let kind = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !FORM_TYPES.contains(&kind.as_str()) {
        return false;
    }
    let Some(origin) = headers.get(ORIGIN).and_then(|value| value.to_str().ok()) else {
        return true;
    };
    match origin_host(origin) {
        Some(origin_host) => !origin_host.eq_ignore_ascii_case(host),
        None => true,
    }
}
