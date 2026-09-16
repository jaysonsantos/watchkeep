//! Test setup. Every test starts with `let _guard = init_goodies();`, so that
//! the tests build the same layers as production. Without a collector the
//! export fails inside the timeout and the tests stay fast.

use std::sync::{Once, OnceLock};

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_error::ErrorLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::propagation::propagator;
use crate::tracing::build_providers;

/// The tracer name of a test run, so that test spans are easy to drop.
const TESTING_TRACER: &str = "testing";

static SETUP: Once = Once::new();
static PROVIDERS: OnceLock<(SdkTracerProvider, SdkMeterProvider)> = OnceLock::new();

/// Flushes both providers when a test ends.
pub struct Flusher;

impl Drop for Flusher {
    fn drop(&mut self) {
        let Some((tracer_provider, meter_provider)) = PROVIDERS.get() else {
            return;
        };
        let _ = tracer_provider.force_flush();
        let _ = meter_provider.force_flush();
    }
}

/// Installs the subscriber, the propagator, the meter provider, and
/// `color_eyre` once per test binary.
pub fn init_goodies() -> Flusher {
    SETUP.call_once(|| {
        let Ok((tracer_provider, meter_provider)) = build_providers() else {
            return;
        };
        global::set_meter_provider(meter_provider.clone());
        global::set_text_map_propagator(propagator());
        let otel_layer = tracing_opentelemetry::layer()
            .with_tracer(tracer_provider.tracer(TESTING_TRACER))
            .with_error_fields_to_exceptions(true)
            .with_error_events_to_status(true)
            .with_error_events_to_exceptions(true)
            .with_error_records_to_exceptions(true);
        let _ = tracing_subscriber::registry()
            .with(tracing_opentelemetry::MetricsLayer::new(
                meter_provider.clone(),
            ))
            .with(otel_layer)
            .with(tracing_subscriber::fmt::layer().with_test_writer())
            .with(ErrorLayer::default())
            .try_init();
        let _ = color_eyre::install();
        let _ = PROVIDERS.set((tracer_provider, meter_provider));
    });
    Flusher
}
