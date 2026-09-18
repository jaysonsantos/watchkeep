//! The end-to-end proof of `watchkeep_telemetry::sqlx`: a real statement on a
//! real server makes a real span, with no attribute on the call site.
//!
//! The test needs a server. `scripts/test-db.sh` starts one and sets
//! `WATCHKEEP_TEST_DATABASE_URL`; without the variable the test does nothing.

use opentelemetry::trace::{SpanKind, TracerProvider as _};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
use opentelemetry_semantic_conventions::attribute::{
    DB_COLLECTION_NAME, DB_OPERATION_NAME, DB_QUERY_SUMMARY, DB_SYSTEM_NAME,
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

#[tokio::test]
async fn a_real_statement_makes_a_span_under_the_caller() {
    let Ok(url) = std::env::var(DATABASE_URL) else {
        eprintln!("no {DATABASE_URL}: the test needs a server");
        return;
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
            Database::postgresql().with_namespace("postgres"),
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
        let one: i32 = sqlx::query_scalar("SELECT 1 FROM pg_catalog.pg_class LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("the statement runs");
        assert_eq!(one, 1);
    }

    provider.force_flush().expect("the spans flush");
    let spans = exporter.get_finished_spans().expect("the spans are there");
    let statement = statement_span(&spans);

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

    let caller = spans
        .iter()
        .find(|span| span.name == CALLER)
        .expect("the caller made a span");
    assert_eq!(statement.parent_span_id, caller.span_context.span_id());
}

/// The span of the statement of the test.
fn statement_span(spans: &[SpanData]) -> &SpanData {
    spans
        .iter()
        .find(|span| span.name == "SELECT pg_catalog.pg_class")
        .unwrap_or_else(|| {
            let names: Vec<&str> = spans.iter().map(|span| span.name.as_ref()).collect();
            panic!("no span of the statement, only {names:?}")
        })
}

fn attribute(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|pair| pair.key.as_str() == key)
        .map(|pair| pair.value.to_string())
}
