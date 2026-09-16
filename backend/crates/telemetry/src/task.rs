//! Named tasks. tokio-console shows the name of a task, and a task that keeps
//! the current span keeps the trace.
//!
//! The macros need `tokio` in the calling crate and the `tokio_unstable` flag
//! from `.cargo/config.toml`.

/// Spawns a named task: `spawn!("plex_sync", future)`. It returns
/// `std::io::Result<tokio::task::JoinHandle<T>>`.
#[macro_export]
macro_rules! spawn {
    ($name:expr, $future:expr $(,)?) => {
        ::tokio::task::Builder::new().name($name).spawn($future)
    };
}

/// Spawns a named blocking closure. It returns
/// `std::io::Result<tokio::task::JoinHandle<T>>`.
#[macro_export]
macro_rules! spawn_blocking {
    ($name:expr, $closure:expr $(,)?) => {
        ::tokio::task::Builder::new()
            .name($name)
            .spawn_blocking($closure)
    };
}
