//! Telemetry of the Watchkeep binary: the subscriber, the OTLP export, the
//! metrics, the trace context propagation, and the error report.
//!
//! The crate holds what does not belong to one service. The HTTP layer that
//! makes a `server` span per request lives in the `watchkeep` crate, next to
//! the router.

pub mod constants;
pub mod propagation;
pub mod report_error;
pub mod sqlx;
pub mod task;
pub mod tracing;

#[cfg(feature = "testing")]
pub mod testing;

pub use constants::{SERVICE_NAME, SERVICE_VERSION};
pub use propagation::{HeaderExtractor, HeaderInjector, PROPAGATOR, context_of, inject_into};
pub use report_error::{error_type_name, init_error_reporting};
pub use sqlx::{Database, Server};
pub use tracing::{EXPORT_GRACE, OtelGuard, configure_tracing, root_span};
