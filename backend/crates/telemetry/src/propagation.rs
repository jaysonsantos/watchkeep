//! W3C trace context and W3C baggage on one propagator, plus the header
//! adapters. Every inject site and every extract site uses this propagator, so
//! that a trace crosses each hop.

use std::sync::LazyLock;

use http::{HeaderMap, HeaderName, HeaderValue};
use opentelemetry::Context;
use opentelemetry::propagation::{
    Extractor, Injector, TextMapCompositePropagator, TextMapPropagator,
};
use opentelemetry_sdk::propagation::{BaggagePropagator, TraceContextPropagator};

/// The propagator of this process. `configure_tracing` registers an equal value
/// as the global propagator, because the global one takes ownership.
pub static PROPAGATOR: LazyLock<TextMapCompositePropagator> = LazyLock::new(propagator);

/// Trace context (`traceparent`, `tracestate`) and baggage (`baggage`).
pub fn propagator() -> TextMapCompositePropagator {
    TextMapCompositePropagator::new(vec![
        Box::new(TraceContextPropagator::new()),
        Box::new(BaggagePropagator::new()),
    ])
}

/// Reads the trace context of an incoming request.
pub struct HeaderExtractor<'a>(pub &'a HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(HeaderName::as_str).collect()
    }
}

/// Writes the trace context into an outgoing request.
pub struct HeaderInjector<'a>(pub &'a mut HeaderMap);

impl Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        let name = HeaderName::from_bytes(key.as_bytes())
            .expect("OpenTelemetry uses header names that are safe");
        if let Ok(value) = HeaderValue::from_str(&value) {
            self.0.insert(name, value);
        }
    }
}

/// The context of the caller, from the headers of an incoming request.
pub fn context_of(headers: &HeaderMap) -> Context {
    PROPAGATOR.extract(&HeaderExtractor(headers))
}

/// Writes `context` into the headers of an outgoing request.
pub fn inject_into(context: &Context, headers: &mut HeaderMap) {
    PROPAGATOR.inject_context(context, &mut HeaderInjector(headers));
}
