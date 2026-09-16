//! Watchkeep: a self-hosted watch tracker with Plex webhook scrobbling.
//!
//! This crate holds the services, the HTTP router, and the importers. The
//! database layers live in `watchkeep-storage` (the Watchkeep database) and
//! `watchkeep-catalog` (the TMDB catalog). `src/main.rs` is the CLI.

pub mod actions;
pub mod app;
pub mod config;
pub mod csrf;
pub mod http;
pub mod plex;
pub mod recommend;
pub mod scrobble;
pub mod telemetry;
pub mod trakt;
pub mod views;

pub use watchkeep_storage::text_enum;
