//! The end-to-end proof of `watchkeep_telemetry::sqlx`: a real statement on a
//! real server makes a real span, with no attribute on the call site.
//!
//! The test needs a server. `scripts/test-db.sh` starts one and sets
//! `WATCHKEEP_TEST_DATABASE_URL`; without the variable the test does nothing.

use opentelemetry::trace::{SpanKind, TracerProvider as _};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
use opentelemetry_semantic_conventions::attribute::{
    DB_COLLECTION_NAME, DB_OPERATION_NAME, DB_QUERY_SUMMARY, DB_QUERY_TEXT, DB_SYSTEM_NAME,
};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::layer::SubscriberExt;
use watchkeep_telemetry::sqlx::{Database, layer};

/// The server of the test. The test does nothing without it.
const DATABASE_URL: &str = "WATCHKEEP_TEST_DATABASE_URL";

/// The tracer name of the test.
const TESTING: &str = "testing";

/// The span of the caller, the parent of every statement span.
const CALLER: &str = "caller";

/// The database name of the server of the test.
const NAMESPACE: &str = "postgres";

#[tokio::test]
async fn a_real_statement_makes_a_span_under_the_caller() {
    let Some(spans) = spans_of("SELECT 1 FROM pg_catalog.pg_class LIMIT 1").await else {
        return;
    };
    let statement = span_named(&spans, "SELECT pg_catalog.pg_class");

    assert_eq!(statement.span_kind, SpanKind::Client);
    assert_eq!(
        attribute(statement, DB_SYSTEM_NAME).as_deref(),
        Some("postgresql")
    );
    assert_eq!(
        attribute(statement, DB_OPERATION_NAME).as_deref(),
        Some("SELECT")
    );
    assert_eq!(
        attribute(statement, DB_COLLECTION_NAME).as_deref(),
        Some("pg_catalog.pg_class")
    );
    assert_eq!(
        attribute(statement, DB_QUERY_SUMMARY).as_deref(),
        Some("SELECT pg_catalog.pg_class")
    );

    let caller = span_named(&spans, CALLER);
    assert_eq!(statement.parent_span_id, caller.span_context.span_id());
}

/// A statement of four words or fewer arrives in the `summary` field alone:
/// sqlx leaves `db.statement` empty for it. sqlx writes both fields as a
/// `String`, which `tracing` records with `record_str`, so the layer reads
/// them. A short statement therefore keeps its name and its text, and never
/// falls back to the name of a statement that gives no operation.
#[tokio::test]
async fn a_short_statement_keeps_its_name_and_its_text() {
    let Some(spans) = spans_of("SELECT 1").await else {
        return;
    };
    let statement = span_named(&spans, "SELECT");

    assert_eq!(
        attribute(statement, DB_QUERY_TEXT).as_deref(),
        Some("SELECT 1")
    );
    assert_eq!(
        attribute(statement, DB_OPERATION_NAME).as_deref(),
        Some("SELECT")
    );
    assert_eq!(
        attribute(statement, DB_QUERY_SUMMARY).as_deref(),
        Some("SELECT")
    );
    // The statement names no table, so the attribute of the table is absent.
    assert_eq!(attribute(statement, DB_COLLECTION_NAME), None);

    let caller = span_named(&spans, CALLER);
    assert_eq!(statement.parent_span_id, caller.span_context.span_id());
}

/// A statement of table expressions does the work of the operation that
/// follows them. The first `FROM` of such a statement belongs to a table
/// expression, so the span must not take its table from there.
#[tokio::test]
async fn a_statement_of_table_expressions_names_its_outer_table() {
    let Some(spans) =
        spans_of("WITH one AS (SELECT 1 AS n FROM pg_catalog.pg_class LIMIT 1) SELECT n FROM one")
            .await
    else {
        return;
    };
    let statement = span_named(&spans, "SELECT one");

    assert_eq!(
        attribute(statement, DB_OPERATION_NAME).as_deref(),
        Some("SELECT")
    );
    assert_eq!(
        attribute(statement, DB_COLLECTION_NAME).as_deref(),
        Some("one")
    );
}

/// Runs one statement under the layer and returns the spans of that run.
/// Returns `None` when the test has no server.
async fn spans_of(statement: &'static str) -> Option<Vec<SpanData>> {
    let Ok(url) = std::env::var(DATABASE_URL) else {
        eprintln!("no {DATABASE_URL}: the test needs a server");
        return None;
    };

    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let tracer = provider.tracer(TESTING);
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(tracer.clone()))
        .with(layer(
            tracer,
            Database::postgresql().with_namespace(NAMESPACE),
        ));

    // The subscriber of this test alone, so that the spans of other tests stay out.
    let dispatch = tracing::Dispatch::new(subscriber);
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("the server answers");

    {
        let _default = tracing::dispatcher::set_default(&dispatch);
        let caller = tracing::info_span!(CALLER);
        let _entered = caller.enter();
        // No attribute, no span, no instrument: the layer alone makes the span.
        let one: i32 = sqlx::query_scalar(statement)
            .fetch_one(&pool)
            .await
            .expect("the statement runs");
        assert_eq!(one, 1);
    }

    provider.force_flush().expect("the spans flush");
    Some(exporter.get_finished_spans().expect("the spans are there"))
}

fn span_named<'a>(spans: &'a [SpanData], name: &str) -> &'a SpanData {
    spans
        .iter()
        .find(|span| span.name == name)
        .unwrap_or_else(|| {
            let names: Vec<&str> = spans.iter().map(|span| span.name.as_ref()).collect();
            panic!("no span named {name:?}, only {names:?}")
        })
}

fn attribute(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|pair| pair.key.as_str() == key)
        .map(|pair| pair.value.to_string())
}
