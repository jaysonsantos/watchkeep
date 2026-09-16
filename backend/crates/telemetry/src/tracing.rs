//! The subscriber of the process: a `fmt` layer on stdout, an OTLP layer for
//! spans, a metrics layer, an optional tokio-console layer, and the error layer
//! of `tracing-error`.
//!
//! `main` calls [`configure_tracing`] first. The OpenTelemetry SDK reads the
//! `OTEL_*` variables itself, so no collector address appears here.

use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use eyre::Result;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::{MetricExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use tracing_error::ErrorLayer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::constants::{SERVICE_NAME, SERVICE_VERSION, SERVICE_VERSION_KEY, defaults, env};
use crate::propagation::propagator;
use crate::report_error::init_error_reporting;

/// The exporters give up after this long, so that a collector that is down
/// never blocks the service.
const EXPORT_TIMEOUT: Duration = Duration::from_secs(2);

/// Time for the batch exporter to send the last spans. `main` waits this long
/// after it drops the guard.
pub const EXPORT_GRACE: Duration = Duration::from_secs(1);

/// Flushes the providers at the end of the process. `main` holds it.
pub struct OtelGuard {
    tracer_provider: SdkTracerProvider,
    meter_provider: SdkMeterProvider,
    flushed: AtomicBool,
}

impl OtelGuard {
    /// Sends what the providers hold. Call it before the process returns. A
    /// second flush waits for the export timeout again, so `Drop` skips its own
    /// flush after this call.
    pub fn force_flush(&self) {
        self.flushed.store(true, Ordering::Relaxed);
        if let Err(error) = self.tracer_provider.force_flush() {
            ::tracing::warn!("cannot flush the spans: {error}");
        }
        if let Err(error) = self.meter_provider.force_flush() {
            ::tracing::warn!("cannot flush the metrics: {error}");
        }
    }
}

impl Drop for OtelGuard {
    /// The last chance of a process that ends without a flush, for example after
    /// a panic.
    fn drop(&mut self) {
        if !self.flushed.load(Ordering::Relaxed) {
            self.force_flush();
        }
    }
}

/// The resource of every span and every metric: the name and the version of
/// this process.
pub fn resource() -> Resource {
    let builder =
        Resource::builder().with_attribute(KeyValue::new(SERVICE_VERSION_KEY, SERVICE_VERSION));
    // `OTEL_SERVICE_NAME` names the process. Without it, every process of this
    // binary reports the same name.
    if std::env::var(env::OTEL_SERVICE_NAME).is_ok() {
        builder.build()
    } else {
        builder.with_service_name(SERVICE_NAME).build()
    }
}

/// The exporters and the providers. The tests build the same pair.
pub(crate) fn build_providers() -> Result<(SdkTracerProvider, SdkMeterProvider)> {
    let span_exporter = SpanExporter::builder()
        .with_http()
        .with_timeout(EXPORT_TIMEOUT)
        .build()?;
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_sampler(Sampler::AlwaysOn)
        .with_resource(resource())
        .build();

    let metric_exporter = MetricExporter::builder()
        .with_http()
        .with_timeout(EXPORT_TIMEOUT)
        .build()?;
    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(resource())
        .build();
    Ok((tracer_provider, meter_provider))
}

/// The `RUST_LOG` filter. The `fmt` layer and the OTLP layer carry it on their own.
fn log_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .with_env_var(env::LOG)
        .try_from_env()
        .unwrap_or_else(|_| EnvFilter::new(defaults::LOG))
}

/// The tokio-console layer, when `CONSOLE_SUBSCRIBER` holds a port.
fn console_layer<S>() -> Option<impl Layer<S>>
where
    S: ::tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a> + Send + Sync,
{
    let port: u16 = std::env::var(env::CONSOLE_SUBSCRIBER).ok()?.parse().ok()?;
    let address: IpAddr = std::env::var(env::CONSOLE_SUBSCRIBER_ADDRESS)
        .unwrap_or_else(|_| defaults::CONSOLE_SUBSCRIBER_ADDRESS.to_owned())
        .parse()
        .ok()?;
    println!("tokio-console listens on {address}:{port}");
    Some(
        console_subscriber::ConsoleLayer::builder()
            .server_addr((address, port))
            .spawn(),
    )
}

/// Installs the subscriber and the error backend. Call it once, first in `main`.
pub fn configure_tracing() -> Result<OtelGuard> {
    let (tracer_provider, meter_provider) = build_providers()?;
    // Without this, `global::meter` returns a no-op meter and the HTTP layer
    // records nothing.
    global::set_meter_provider(meter_provider.clone());
    global::set_text_map_propagator(propagator());

    let otel_layer = tracing_opentelemetry::layer()
        .with_tracer(tracer_provider.tracer(SERVICE_NAME))
        .with_error_fields_to_exceptions(true)
        .with_error_events_to_status(true)
        .with_error_events_to_exceptions(true)
        .with_error_records_to_exceptions(true);
    // The metrics layer carries `INFO`, not `RUST_LOG`: a metric field on an
    // `info!` event must still count when an operator sets `RUST_LOG=warn`. A
    // metric therefore goes on an `info!` event, never on a `debug!` event.
    let metrics_layer = tracing_opentelemetry::MetricsLayer::new(meter_provider.clone())
        .with_filter(LevelFilter::INFO);

    // Each layer filters itself. A filter on the registry would cut the trace
    // events of tokio that tokio-console needs, and the events of the metrics.
    tracing_subscriber::registry()
        .with(console_layer())
        .with(metrics_layer)
        .with(otel_layer.with_filter(log_filter()))
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_filter(log_filter()),
        )
        .with(ErrorLayer::default())
        .init();
    init_error_reporting()?;
    Ok(OtelGuard {
        tracer_provider,
        meter_provider,
        flushed: AtomicBool::new(false),
    })
}

/// The root span of a command. Every trace of the process descends from it.
pub fn root_span(command: &str) -> ::tracing::Span {
    ::tracing::info_span!(SERVICE_NAME, command = command, version = SERVICE_VERSION)
}
