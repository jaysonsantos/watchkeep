//! The HTTP telemetry of the server: one `server` span per request, the trace
//! context of the caller, and the request metrics. The parts that no service
//! owns live in `watchkeep-telemetry`.

use std::time::{Duration, Instant};

use axum::extract::{MatchedPath, Request, State};
use axum::http::header::{CONTENT_TYPE, ORIGIN, USER_AGENT};
use axum::http::{HeaderMap, HeaderName, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::metrics::{Counter, Histogram, UpDownCounter};
use opentelemetry::{KeyValue, global};
use tracing::{Instrument, Span, field};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use watchkeep_telemetry::SERVICE_NAME;
use watchkeep_telemetry::propagation::context_of;

/// The span of one HTTP request. `otel.name` carries the method and the route.
const SPAN_NAME: &str = "watchkeep_http";

/// The route label of a request that no route matched, for example a file of
/// the web UI. A path would raise the cardinality of the metrics.
const UNMATCHED_ROUTE: &str = "unmatched";

/// The methods that get their own label. Every other method is `OTHER_METHOD`,
/// so that a request with a rare method cannot raise the cardinality.
const KNOWN_METHODS: [&str; 7] = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];
const OTHER_METHOD: &str = "other";

/// The `status` label: an answer below 500 did what the caller asked.
const STATUS_SUCCESS: &str = "success";
const STATUS_ERROR: &str = "error";

/// The kind of every span of this layer, as a label of the metrics.
const SERVER_KIND: &str = "server";

/// A duration in milliseconds, from `Duration::as_secs_f64`.
const MILLISECONDS_PER_SECOND: f64 = 1_000.0;

/// The names of the instruments. One prefix, snake_case, a unit each.
mod instrument {
    pub const REQUESTS: &str = "watchkeep_http_requests_total";
    pub const DURATION: &str = "watchkeep_http_request_duration_ms";
    pub const ACTIVE: &str = "watchkeep_http_active_requests";
    pub const UNIT_REQUESTS: &str = "requests";
    pub const UNIT_MILLISECONDS: &str = "ms";
}

/// The labels of the instruments. Every value is low cardinality.
mod label {
    pub const METHOD: &str = "http.method";
    pub const ROUTE: &str = "http.route";
    pub const STATUS: &str = "status";
    pub const KIND: &str = "otel.kind";
}

/// The request headers that go on the span.
const TRACEPARENT: &str = "traceparent";

/// The instruments of the HTTP layer. They are built once, from the global meter.
#[derive(Clone)]
pub struct HttpMetrics {
    requests: Counter<u64>,
    duration: Histogram<f64>,
    active: UpDownCounter<i64>,
}

impl Default for HttpMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpMetrics {
    /// Builds the instruments. `configure_tracing` must have registered the
    /// meter provider before, else the meter is a no-op.
    pub fn new() -> Self {
        let meter = global::meter(SERVICE_NAME);
        Self {
            requests: meter
                .u64_counter(instrument::REQUESTS)
                .with_description("HTTP requests that the server answered.")
                .with_unit(instrument::UNIT_REQUESTS)
                .build(),
            duration: meter
                .f64_histogram(instrument::DURATION)
                .with_description("Time from the request to the answer.")
                .with_unit(instrument::UNIT_MILLISECONDS)
                .build(),
            active: meter
                .i64_up_down_counter(instrument::ACTIVE)
                .with_description("HTTP requests in flight.")
                .with_unit(instrument::UNIT_REQUESTS)
                .build(),
        }
    }
}

/// The value of one header, when it is text.
fn header_of<'a>(headers: &'a HeaderMap, name: &HeaderName) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// Puts the headers that help to read a trace on the span.
fn record_headers(span: &Span, headers: &HeaderMap) {
    if let Some(value) = header_of(headers, &USER_AGENT) {
        span.record("http.request.header.user_agent", value);
    }
    if let Some(value) = header_of(headers, &CONTENT_TYPE) {
        span.record("http.request.header.content_type", value);
    }
    if let Some(value) = header_of(headers, &ORIGIN) {
        span.record("http.request.header.origin", value);
    }
    if let Some(value) = headers
        .get(TRACEPARENT)
        .and_then(|value| value.to_str().ok())
    {
        span.record("http.request.header.traceparent", value);
    }
}

/// The `server` span of one request, its metrics, and the trace of the caller.
/// The route comes from the matched path, so that a label stays low cardinality.
pub async fn trace_request(
    State(metrics): State<HttpMetrics>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().to_string();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or(UNMATCHED_ROUTE, MatchedPath::as_str)
        .to_owned();
    let name = format!("{method} {route}");
    let span = tracing::info_span!(
        SPAN_NAME,
        otel.name = %name,
        otel.kind = SERVER_KIND,
        http.method = %method,
        http.route = %route,
        // The path only. The query string of the webhook carries the token, and
        // a span must never export a secret.
        http.url = %request.uri().path(),
        http.status_code = field::Empty,
        http.request.header.user_agent = field::Empty,
        http.request.header.content_type = field::Empty,
        http.request.header.origin = field::Empty,
        http.request.header.traceparent = field::Empty,
    );
    record_headers(&span, request.headers());
    // Without an OpenTelemetry layer, for example in a unit test, there is no
    // parent to set.
    if let Err(error) = span.set_parent(context_of(request.headers())) {
        tracing::debug!("cannot read the trace context of the caller: {error}");
    }

    let mut labels = vec![
        KeyValue::new(label::METHOD, method_label(&method)),
        KeyValue::new(label::ROUTE, route),
        KeyValue::new(label::KIND, SERVER_KIND),
    ];
    let active = ActiveRequest::new(metrics.active.clone(), labels.clone());
    let started = Instant::now();
    let response = next.run(request).instrument(span.clone()).await;
    let elapsed = started.elapsed();
    drop(active);

    let status = response.status();
    span.record("http.status_code", status.as_u16());
    labels.push(KeyValue::new(label::STATUS, status_label(status)));
    metrics.requests.add(1, &labels);
    metrics.duration.record(milliseconds(elapsed), &labels);
    response
}

/// One request in flight. It counts down on drop, so a request that the client
/// cancels, and a handler that panics, leave the counter balanced.
struct ActiveRequest {
    counter: UpDownCounter<i64>,
    labels: Vec<KeyValue>,
}

impl ActiveRequest {
    fn new(counter: UpDownCounter<i64>, labels: Vec<KeyValue>) -> Self {
        counter.add(1, &labels);
        Self { counter, labels }
    }
}

impl Drop for ActiveRequest {
    fn drop(&mut self) {
        self.counter.add(-1, &self.labels);
    }
}

/// The label of a method: the method itself when it is a known one, else `other`.
fn method_label(method: &str) -> &'static str {
    KNOWN_METHODS
        .into_iter()
        .find(|known| *known == method)
        .unwrap_or(OTHER_METHOD)
}

/// `error` for an answer that the server could not produce, else `success`.
fn status_label(status: StatusCode) -> &'static str {
    if status.is_server_error() {
        STATUS_ERROR
    } else {
        STATUS_SUCCESS
    }
}

/// A duration as milliseconds, for a histogram.
pub fn milliseconds(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * MILLISECONDS_PER_SECOND
}
