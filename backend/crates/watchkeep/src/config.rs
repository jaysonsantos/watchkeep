//! The command line and the configuration. Every setting is a flag with an
//! environment variable behind it, so `watchkeep --help` documents both.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use watchkeep_catalog::CatalogLanguage;

/// The environment variables that Watchkeep reads.
pub mod env {
    pub const HOST: &str = "WATCHKEEP_HOST";
    pub const PORT: &str = "WATCHKEEP_PORT";
    pub const DATABASE_URL: &str = "WATCHKEEP_DATABASE_URL";
    pub const CATALOG_DATABASE_URL: &str = "WATCHKEEP_CATALOG_DATABASE_URL";
    pub const CATALOG_LANGUAGE: &str = "WATCHKEEP_CATALOG_LANGUAGE";
    pub const IMAGE_BASE_URL: &str = "WATCHKEEP_IMAGE_BASE_URL";
    pub const WEBHOOK_TOKEN: &str = "WATCHKEEP_WEBHOOK_TOKEN";
    pub const WATCHED_THRESHOLD_PERCENT: &str = "WATCHKEEP_WATCHED_THRESHOLD_PERCENT";
    pub const REWATCH_WINDOW_MINUTES: &str = "WATCHKEEP_REWATCH_WINDOW_MINUTES";
    pub const PLEX_ACCOUNTS: &str = "WATCHKEEP_PLEX_ACCOUNTS";
    pub const PLEX_URL: &str = "WATCHKEEP_PLEX_URL";
    pub const PLEX_TOKEN: &str = "WATCHKEEP_PLEX_TOKEN";
    pub const SYNC_INTERVAL_MINUTES: &str = "WATCHKEEP_SYNC_INTERVAL_MINUTES";
    pub const WEBHOOK_RETENTION_DAYS: &str = "WATCHKEEP_WEBHOOK_RETENTION_DAYS";
    pub const STATIC_DIR: &str = "WATCHKEEP_STATIC_DIR";
    pub const HTTP_ORIGIN: &str = "WATCHKEEP_HTTP_ORIGIN";
    pub const HOST_HEADER: &str = "WATCHKEEP_HTTP_HOST_HEADER";
}

/// The names of the commands. The root span of the process records one of them.
pub mod command {
    pub const SERVE: &str = "serve";
    pub const SYNC: &str = "sync";
    pub const TRAKT_IMPORT: &str = "trakt:import";
    pub const HEALTH: &str = "health";
}

/// The values that apply when a flag and its variable are absent.
pub mod defaults {
    use watchkeep_catalog::CatalogLanguage;

    pub const HOST: &str = "0.0.0.0";
    pub const PORT: u16 = 8484;
    pub const DATABASE_URL: &str = "postgres://watchkeep:watchkeep@localhost:5432/watchkeep";
    pub const CATALOG_LANGUAGE: CatalogLanguage = CatalogLanguage::En;
    pub const IMAGE_BASE_URL: &str = "https://image.tmdb.org/t/p/";
    pub const WATCHED_THRESHOLD_PERCENT: f64 = 85.0;
    /// Minutes, as the flag reads them.
    pub const REWATCH_WINDOW_MINUTES: &str = "360";
    /// Minutes. Zero disables the timer.
    pub const SYNC_INTERVAL_MINUTES: &str = "0";
    /// Days. Zero keeps every event.
    pub const WEBHOOK_RETENTION_DAYS: &str = "30";
    pub const STATIC_DIR: &str = "frontend/build";
    pub const HOST_HEADER: &str = "host";
}

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_DAY: u64 = 24 * 60 * SECONDS_PER_MINUTE;

/// Separator of `WATCHKEEP_PLEX_ACCOUNTS`.
const LIST_SEPARATOR: char = ',';

fn duration_of(text: &str, unit_seconds: u64, unit: &str) -> Result<Duration, String> {
    let value: f64 = text
        .trim()
        .parse()
        .map_err(|_| format!("{text} is not a number of {unit}"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("{text} is not a number of {unit}"));
    }
    Ok(Duration::from_secs_f64(value * unit_seconds as f64))
}

fn minutes(text: &str) -> Result<Duration, String> {
    duration_of(text, SECONDS_PER_MINUTE, "minutes")
}

fn days(text: &str) -> Result<Duration, String> {
    duration_of(text, SECONDS_PER_DAY, "days")
}

/// A base URL that always ends with one `/`.
fn base_url(text: &str) -> Result<String, String> {
    Ok(format!("{}/", text.trim().trim_end_matches('/')))
}

/// A URL without a trailing `/`. Empty stays empty.
fn plain_url(text: &str) -> Result<String, String> {
    Ok(text.trim().trim_end_matches('/').to_owned())
}

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Listen address.
    #[arg(long, env = env::HOST, default_value = defaults::HOST)]
    pub host: String,

    /// Listen port.
    #[arg(long, env = env::PORT, default_value_t = defaults::PORT)]
    pub port: u16,

    /// Postgres connection string of the Watchkeep database.
    #[arg(long, env = env::DATABASE_URL, default_value = defaults::DATABASE_URL)]
    pub database_url: String,

    /// Postgres connection string of the TMDB catalog database. Empty disables the catalog.
    #[arg(long, env = env::CATALOG_DATABASE_URL, default_value = "")]
    pub catalog_database_url: String,

    /// Which catalog title to prefer: en or pt.
    #[arg(long, env = env::CATALOG_LANGUAGE, default_value_t = defaults::CATALOG_LANGUAGE)]
    pub catalog_language: CatalogLanguage,

    /// Base URL for TMDB poster paths.
    #[arg(long, env = env::IMAGE_BASE_URL, default_value = defaults::IMAGE_BASE_URL, value_parser = base_url)]
    pub image_base_url: String,

    /// Token that Plex must send as `?token=` on the webhook URL. Empty accepts any caller.
    #[arg(long, env = env::WEBHOOK_TOKEN, default_value = "")]
    pub webhook_token: String,

    /// A stop event at or above this percentage records a play.
    #[arg(long, env = env::WATCHED_THRESHOLD_PERCENT, default_value_t = defaults::WATCHED_THRESHOLD_PERCENT)]
    pub watched_threshold_percent: f64,

    /// Two plays of one item inside this window count as one play, in minutes.
    #[arg(long, env = env::REWATCH_WINDOW_MINUTES, default_value = defaults::REWATCH_WINDOW_MINUTES, value_parser = minutes)]
    pub rewatch_window: Duration,

    /// Plex account titles or ids to accept, comma-separated. Empty accepts every account.
    #[arg(long, env = env::PLEX_ACCOUNTS, value_delimiter = LIST_SEPARATOR)]
    pub plex_accounts: Vec<String>,

    /// Plex server URL for the library sync, for example http://plex:32400.
    #[arg(long, env = env::PLEX_URL, default_value = "", value_parser = plain_url)]
    pub plex_url: String,

    /// Plex token for the library sync.
    #[arg(long, env = env::PLEX_TOKEN, default_value = "")]
    pub plex_token: String,

    /// Run a library sync on this interval, in minutes. Zero disables the timer.
    #[arg(long, env = env::SYNC_INTERVAL_MINUTES, default_value = defaults::SYNC_INTERVAL_MINUTES, value_parser = minutes)]
    pub sync_interval: Duration,

    /// Keep raw webhook events for this many days. Zero keeps them.
    #[arg(long, env = env::WEBHOOK_RETENTION_DAYS, default_value = defaults::WEBHOOK_RETENTION_DAYS, value_parser = days)]
    pub webhook_retention: Duration,

    /// Directory with the built web UI.
    #[arg(long, env = env::STATIC_DIR, default_value = defaults::STATIC_DIR)]
    pub static_dir: PathBuf,

    /// Public URL, for example https://watchkeep.example.com. Set it behind a proxy that changes the Host header.
    #[arg(long, env = env::HTTP_ORIGIN, default_value = "")]
    pub http_origin: String,

    /// Header that carries the host of the request, for example x-forwarded-host.
    #[arg(long, env = env::HOST_HEADER, default_value = defaults::HOST_HEADER)]
    pub host_header: String,
}

impl Default for Config {
    /// The defaults, without the environment. Tests start from here.
    fn default() -> Self {
        Self {
            host: defaults::HOST.to_owned(),
            port: defaults::PORT,
            database_url: defaults::DATABASE_URL.to_owned(),
            catalog_database_url: String::new(),
            catalog_language: defaults::CATALOG_LANGUAGE,
            image_base_url: base_url(defaults::IMAGE_BASE_URL).expect("a URL"),
            webhook_token: String::new(),
            watched_threshold_percent: defaults::WATCHED_THRESHOLD_PERCENT,
            rewatch_window: minutes(defaults::REWATCH_WINDOW_MINUTES).expect("a number"),
            plex_accounts: Vec::new(),
            plex_url: String::new(),
            plex_token: String::new(),
            sync_interval: minutes(defaults::SYNC_INTERVAL_MINUTES).expect("a number"),
            webhook_retention: days(defaults::WEBHOOK_RETENTION_DAYS).expect("a number"),
            static_dir: PathBuf::from(defaults::STATIC_DIR),
            http_origin: String::new(),
            host_header: defaults::HOST_HEADER.to_owned(),
        }
    }
}

impl Config {
    pub fn sync_configured(&self) -> bool {
        !self.plex_url.is_empty() && !self.plex_token.is_empty()
    }

    /// The accepted Plex accounts, without blanks. Empty accepts every account.
    pub fn accepted_accounts(&self) -> Vec<&str> {
        self.plex_accounts
            .iter()
            .map(|account| account.trim())
            .filter(|account| !account.is_empty())
            .collect()
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "watchkeep",
    version,
    about = "Self-hosted watch tracker with Plex webhook scrobbling."
)]
pub struct Cli {
    #[command(flatten)]
    pub config: Config,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Clone, Debug)]
pub enum Command {
    /// Serve the web UI, the JSON API, and the Plex webhook. The default.
    Serve,
    /// Run a Plex library sync and print the report as JSON.
    Sync,
    /// Import a Trakt data export, the ZIP from trakt.tv, and print the report as JSON.
    #[command(name = command::TRAKT_IMPORT)]
    TraktImport {
        /// Path of the export ZIP.
        zip: PathBuf,
        /// Count the records without a write.
        #[arg(long)]
        dry_run: bool,
    },
    /// Ask the running server on this port for its health. The Docker HEALTHCHECK runs it.
    Health,
}

impl Command {
    /// The name of the command, as the root span records it.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Serve => command::SERVE,
            Self::Sync => command::SYNC,
            Self::TraktImport { .. } => command::TRAKT_IMPORT,
            Self::Health => command::HEALTH,
        }
    }
}
