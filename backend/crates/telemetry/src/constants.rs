//! The names, the version, and the variables that the telemetry code uses.

/// The tracer name and the prefix of every instrument. `OTEL_SERVICE_NAME`
/// names the service in the collector; this name stays the same.
pub const SERVICE_NAME: &str = "watchkeep";

/// The version of the build, from the manifest. It is the `service.version`
/// attribute of the resource and a field of the root span.
pub const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The resource attribute that carries [`SERVICE_VERSION`].
pub const SERVICE_VERSION_KEY: &str = "service.version";

/// The environment variables that the telemetry code reads itself. The
/// OpenTelemetry SDK reads the `OTEL_*` variables on its own.
pub mod env {
    /// The name of this process in the collector. The OpenTelemetry SDK reads it.
    pub const OTEL_SERVICE_NAME: &str = "OTEL_SERVICE_NAME";
    /// Log filter of the `fmt` layer and the OTLP layer.
    pub const LOG: &str = "RUST_LOG";
    /// A port. When it is set, the tokio-console layer starts.
    pub const CONSOLE_SUBSCRIBER: &str = "CONSOLE_SUBSCRIBER";
    /// Bind address of the tokio-console layer.
    pub const CONSOLE_SUBSCRIBER_ADDRESS: &str = "CONSOLE_SUBSCRIBER_ADDRESS";
}

/// The values that apply when a variable is absent.
pub mod defaults {
    pub const LOG: &str = "info";
    pub const CONSOLE_SUBSCRIBER_ADDRESS: &str = "127.0.0.1";
}
