//! One way to report an error. `report_error!` emits an event that the
//! OpenTelemetry layer turns into a span exception, and that sets the status of
//! the span to error.

use eyre::Result;

/// The name of the type of an error, without a leading `&`.
pub fn error_type_name<E: ?Sized>(_error: &E) -> &'static str {
    std::any::type_name::<E>().trim_start_matches('&')
}

/// Installs the error backend. `configure_tracing` calls it. It stays a
/// separate function, so that another backend can attach later.
pub fn init_error_reporting() -> Result<()> {
    color_eyre::install()?;
    Ok(())
}

/// Reports an error once, at the layer that handles it.
///
/// It emits an `ERROR` event with the target `exception` and the fields
/// `exception.type`, `exception.message`, and `exception.stacktrace`. The
/// OpenTelemetry layer runs with `with_error_events_to_status` and
/// `with_error_events_to_exceptions`, so the event sets the status of the
/// current span and becomes a span exception. Callers that pass the error on
/// with `?` do not report it.
#[macro_export]
macro_rules! report_error {
    ($message:expr, $error:expr $(,)?) => {{
        let error = &$error;
        // `event!` with a target, because the field names are quoted: `type` is
        // a keyword, so `exception.type` cannot be an identifier.
        ::tracing::event!(
            target: "exception",
            ::tracing::Level::ERROR,
            "exception.type" = $crate::report_error::error_type_name(error),
            "exception.message" = %error,
            "exception.stacktrace" = ?error,
            "{}",
            $message
        );
    }};
}
