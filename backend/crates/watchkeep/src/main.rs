//! CLI entry point. See `config::Cli` for the commands and the flags.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use color_eyre::eyre::{Result, WrapErr, eyre};
use tracing::Instrument;
use watchkeep::app::{AppContext, SharedContext, build_router, has_web_ui};
use watchkeep::config::{Cli, Command, Config, env};
use watchkeep::trakt::import::{ImportOptions, import_trakt_export};
use watchkeep_storage::db::{create_pool, open_database};
use watchkeep_telemetry::tracing::EXPORT_GRACE;
use watchkeep_telemetry::{configure_tracing, report_error, root_span, spawn};

/// Pool size of the catalog database.
const CATALOG_POOL_SIZE: u32 = 3;

/// The first timed sync waits this long after start-up.
const FIRST_SYNC_DELAY: Duration = Duration::from_secs(5);

/// The health check gives up after this long.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);

/// The health check calls the server on the loopback address, whatever `--host` is.
const LOOPBACK: &str = "127.0.0.1";

/// The name of the task that runs the timed sync. tokio-console shows it.
const SYNC_TIMER_TASK: &str = "sync_timer";

/// `main` returns the exit code, not the error, because it reports the error
/// itself. A `Result` would print the same report a second time.
#[tokio::main]
async fn main() -> ExitCode {
    // First of all, so that every later line has a subscriber and a span.
    let guard = match configure_tracing() {
        Ok(guard) => guard,
        Err(error) => {
            // No subscriber yet, so this is the only way to say it.
            eprintln!("cannot configure the telemetry: {error:?}");
            return ExitCode::FAILURE;
        }
    };
    let code = match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report_error!("the command failed", error);
            ExitCode::FAILURE
        }
    };
    guard.force_flush();
    drop(guard);
    // The batch exporter sends on its own thread. Give it time before the exit.
    tokio::time::sleep(EXPORT_GRACE).await;
    code
}

/// Reads the command line and runs the command inside the root span.
async fn run() -> Result<()> {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::Serve);
    let span = root_span(command.name());
    match command {
        Command::Serve => serve(cli.config).instrument(span).await,
        Command::Sync => sync(cli.config).instrument(span).await,
        Command::TraktImport { zip, dry_run } => {
            trakt_import(cli.config, zip, dry_run)
                .instrument(span)
                .await
        }
        Command::Health => health(&cli.config).instrument(span).await,
    }
}

/// Runs one Plex library sync and prints the report as JSON.
async fn sync(config: Config) -> Result<()> {
    let ctx = open_context(config).await?;
    let report = ctx.sync().await.map_err(|error| eyre!("{error:#}"))?;
    println!("{}", serde_json::to_string(&report)?);
    ctx.close().await;
    Ok(())
}

/// Imports a Trakt export and prints the report as JSON.
async fn trakt_import(config: Config, zip: std::path::PathBuf, dry_run: bool) -> Result<()> {
    let ctx = open_context(config).await?;
    let options = ImportOptions {
        clock: ctx.clock.clone(),
        dry_run,
    };
    let report = import_trakt_export(&ctx.pool, ctx.catalog.as_ref(), &zip, options).await?;
    println!("{}", serde_json::to_string(&report)?);
    ctx.close().await;
    Ok(())
}

/// Exit code 0 when the server answers `/healthz` with success, else an error.
async fn health(config: &Config) -> Result<()> {
    let url = format!(
        "http://{LOOPBACK}:{}{}",
        config.port,
        watchkeep::app::HEALTH_PATH
    );
    let response = reqwest::Client::builder()
        .timeout(HEALTH_TIMEOUT)
        .build()?
        .get(&url)
        .send()
        .await
        .wrap_err_with(|| format!("no answer from {url}"))?;
    if !response.status().is_success() {
        return Err(eyre!("{url} answered {}", response.status()));
    }
    println!("{}", response.text().await?);
    Ok(())
}

async fn open_context(config: Config) -> Result<SharedContext> {
    let pool = open_database(&config.database_url)
        .await
        .wrap_err("cannot open the Watchkeep database")?;
    let catalog_pool = if config.catalog_database_url.is_empty() {
        None
    } else {
        Some(
            create_pool(&config.catalog_database_url, CATALOG_POOL_SIZE)
                .await
                .wrap_err("cannot open the catalog database")?,
        )
    };
    Ok(Arc::new(AppContext::new(pool, catalog_pool, config, None)))
}

async fn serve(config: Config) -> Result<()> {
    let ctx = open_context(config).await?;
    if ctx.catalog.is_some() {
        tracing::info!("catalog enabled");
    } else {
        tracing::info!(
            "catalog disabled (set {} to enable)",
            env::CATALOG_DATABASE_URL
        );
    }
    if ctx.config.webhook_token.is_empty() {
        tracing::warn!(
            "{} is empty, the webhook accepts any caller",
            env::WEBHOOK_TOKEN
        );
    }
    if !has_web_ui(&ctx.config.static_dir) {
        tracing::warn!(
            "no web UI at {}: build it with `pnpm build` in frontend/, or set {}",
            ctx.config.static_dir.display(),
            env::STATIC_DIR
        );
    }

    let mut timer = None;
    if !ctx.config.sync_interval.is_zero() && ctx.config.sync_configured() {
        let every = ctx.config.sync_interval;
        let ctx = ctx.clone();
        let task = async move {
            tokio::time::sleep(FIRST_SYNC_DELAY).await;
            loop {
                // One span per run, because each run is one unit of work.
                let span = tracing::info_span!("sync_timer", otel.kind = "consumer");
                if let Err(error) = ctx.sync().instrument(span).await {
                    report_error!("the timed sync failed", error);
                }
                tokio::time::sleep(every).await;
            }
        };
        timer = Some(spawn!(SYNC_TIMER_TASK, task).wrap_err("cannot start the sync timer")?);
    }

    let address = format!("{}:{}", ctx.config.host, ctx.config.port);
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .wrap_err_with(|| format!("cannot listen on {address}"))?;
    tracing::info!("listening on http://{address}");
    axum::serve(listener, build_router(ctx.clone()))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("shutting down");
    if let Some(timer) = timer {
        timer.abort();
    }
    ctx.close().await;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
