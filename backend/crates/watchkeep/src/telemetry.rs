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
use sqlx::postgres::PgConnectOptions;

use crate::config::Config;
use tracing::{Instrument, Span, field};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use watchkeep_telemetry::propagation::context_of;
use watchkeep_telemetry::{Database, SERVICE_NAME, Server};

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
    // The name of a span groups the traces of a backend, so it takes the bounded
    // method. The field `http.method` keeps the method as it arrived.
    let name = format!("{} {route}", method_label(&method));
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

// region: database

/// Names the server of the statement spans. The subscriber starts before this
/// process reads its command line, so the layer learns the server here.
///
/// One layer serves every pool of the process, and the statement event names no
/// pool, so an attribute must hold for all of them. The catalog can live on
/// another server or in another database, so each attribute goes in only when
/// every pool agrees on it. A wrong server is worse than none.
///
/// The settings come from the driver, never from the URL as text: the URL
/// carries the password, and a span must never hold it. A URL that the driver
/// cannot read leaves the server without a name; the pool reports that failure
/// itself.
pub fn describe_database(database: &Database, config: &Config) {
    let Some(pools) = connection_options(config) else {
        return;
    };
    let Some(first) = pools.first() else {
        return;
    };

    let mut server = Server::default();
    if pools
        .iter()
        .all(|options| namespace_of(options) == namespace_of(first))
    {
        server = server.with_namespace(namespace_of(first));
    }
    let same_server = pools.iter().all(|options| {
        options.get_host() == first.get_host() && options.get_port() == first.get_port()
    });
    if same_server && let Some(default_port) = database.default_port() {
        server = server.with_address(first.get_host(), first.get_port(), default_port);
    }
    database.describe(server);
}

/// The settings of every pool that this process opens. `None` when a URL that
/// the process uses is one that the driver cannot read.
fn connection_options(config: &Config) -> Option<Vec<PgConnectOptions>> {
    let mut urls = vec![config.database_url.as_str()];
    if !config.catalog_database_url.is_empty() {
        urls.push(config.catalog_database_url.as_str());
    }
    urls.into_iter()
        .map(|url| url.parse::<PgConnectOptions>().ok())
        .collect()
}

/// The database that a connection lands in. PostgreSQL takes the name of the
/// user when the settings name no database, and sqlx sends no name in that
/// case, so the name of the user is the answer.
fn namespace_of(options: &PgConnectOptions) -> &str {
    options
        .get_database()
        .unwrap_or_else(|| options.get_username())
}

#[cfg(test)]
mod database_tests {
    use super::*;
    use crate::config::Config;

    /// A port that is not the default one of PostgreSQL.
    const OTHER_PORT: u16 = 6432;

    fn server_of(database_url: &str, catalog_database_url: &str) -> Server {
        let config = Config {
            database_url: database_url.to_owned(),
            catalog_database_url: catalog_database_url.to_owned(),
            ..Config::default()
        };
        let database = Database::postgresql();
        describe_database(&database, &config);
        database.server().cloned().unwrap_or_default()
    }

    /// One pool alone: every attribute holds.
    fn server_of_one(url: &str) -> Server {
        server_of(url, "")
    }

    #[test]
    fn the_url_names_the_database_and_the_host() {
        let server = server_of_one("postgres://someone:secret@db.example:6432/watchkeep");
        assert_eq!(server.namespace.as_deref(), Some("watchkeep"));
        assert_eq!(server.address.as_deref(), Some("db.example"));
        assert_eq!(server.port, Some(OTHER_PORT));
    }

    /// The conventions ask for the port only when it is not the default one.
    #[test]
    fn the_default_port_stays_out() {
        let server = server_of_one("postgres://someone:secret@db.example/watchkeep");
        assert_eq!(server.address.as_deref(), Some("db.example"));
        assert_eq!(server.port, None);
    }

    /// PostgreSQL takes the name of the user when the URL names no database.
    #[test]
    fn a_url_without_a_database_lands_in_the_database_of_the_user() {
        let server = server_of_one("postgres://watchkeep:secret@db.example");
        assert_eq!(server.namespace.as_deref(), Some("watchkeep"));
    }

    /// The URL carries the password, and no attribute may hold it.
    #[test]
    fn no_attribute_holds_the_password() {
        let server = server_of_one("postgres://someone:secret@db.example:6432/watchkeep");
        for value in [&server.namespace, &server.address] {
            assert!(
                !value.as_deref().unwrap_or_default().contains("secret"),
                "an attribute holds the password: {value:?}"
            );
        }
    }

    /// A URL that the driver cannot read leaves the server without a name. The
    /// pool reports that failure itself.
    #[test]
    fn a_url_that_no_driver_reads_names_nothing() {
        assert_eq!(server_of_one("this is no url"), Server::default());
        assert_eq!(
            server_of("postgres://someone@db.example/watchkeep", "this is no url"),
            Server::default()
        );
    }

    /// The catalog of the same server but another database: the host holds for
    /// both pools, the name of the database does not.
    #[test]
    fn a_catalog_of_another_database_drops_the_namespace() {
        let server = server_of(
            "postgres://someone:secret@db.example:6432/watchkeep",
            "postgres://someone:secret@db.example:6432/catalog",
        );
        assert_eq!(server.namespace, None);
        assert_eq!(server.address.as_deref(), Some("db.example"));
        assert_eq!(server.port, Some(OTHER_PORT));
    }

    /// The catalog of another server: no attribute of a server holds.
    #[test]
    fn a_catalog_of_another_server_drops_the_address() {
        let server = server_of(
            "postgres://someone:secret@db.example:6432/watchkeep",
            "postgres://someone:secret@catalog.example:6432/watchkeep",
        );
        assert_eq!(server.address, None);
        assert_eq!(server.port, None);
        // Both pools land in a database of the same name.
        assert_eq!(server.namespace.as_deref(), Some("watchkeep"));
    }

    /// The catalog of the same database: every attribute holds.
    #[test]
    fn a_catalog_of_the_same_database_keeps_everything() {
        let url = "postgres://someone:secret@db.example:6432/watchkeep";
        let server = server_of(url, url);
        assert_eq!(server.namespace.as_deref(), Some("watchkeep"));
        assert_eq!(server.address.as_deref(), Some("db.example"));
        assert_eq!(server.port, Some(OTHER_PORT));
    }
}

// endregion: database
