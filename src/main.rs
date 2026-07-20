use clap::Parser;
use tokio::net::TcpListener;
use tracing_subscriber::{
    EnvFilter, Layer as _, fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt,
};

mod config;
mod db;
mod error;
mod handlers;
mod middleware;
mod migrations;
mod models;
mod repositories;
mod routes;
mod session;
mod state;
mod version;

use config::Config;
use migrations::run_migrations;
use repositories::{ExerciseRepository, SessionRepository, UserRepository, WorkoutRepository};
use state::AppState;

// The release image links musl, whose default allocator is markedly slower than
// glibc's under concurrent load. mimalloc restores throughput for the request
// handlers and the r2d2 SQLite pool.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
enum LogFormat {
    #[default]
    Full,
    Compact,
    Pretty,
    Json,
}

#[derive(Parser, Debug)]
#[command(name = "liftlog")]
struct Args {
    /// Log output format
    #[arg(long, env = "LOG_FORMAT", default_value = "full")]
    log_format: LogFormat,
}

fn init_tracing(format: LogFormat) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("error,liftlog=info"));
    let span_events = env_filter.max_level_hint().map_or(FmtSpan::CLOSE, |l| {
        if l >= tracing::Level::DEBUG {
            FmtSpan::CLOSE
        } else {
            FmtSpan::NONE
        }
    });
    let use_ansi = std::env::var_os("NO_COLOR").is_none();
    let layer = tracing_subscriber::fmt::layer()
        .with_span_events(span_events)
        .with_ansi(use_ansi);
    let layer = match format {
        LogFormat::Full => layer.with_filter(env_filter).boxed(),
        LogFormat::Compact => layer.compact().with_filter(env_filter).boxed(),
        LogFormat::Pretty => layer.pretty().with_filter(env_filter).boxed(),
        LogFormat::Json => layer.json().with_filter(env_filter).boxed(),
    };
    tracing_subscriber::registry().with(layer).init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    init_tracing(args.log_format);

    // Load environment variables
    dotenvy::dotenv().ok();

    // Load configuration
    let config = Config::from_env()?;

    tracing::info!("Connecting to database: {}", config.database_url);

    // Create database pool
    let pool = db::create_pool(&config.database_url)?;

    // Run migrations
    run_migrations(&pool)?;

    // Create repositories
    let user_repo = UserRepository::new(pool.clone());
    let exercise_repo = ExerciseRepository::new(pool.clone());
    let workout_repo = WorkoutRepository::new(pool.clone());
    let session_repo = SessionRepository::new(pool.clone());

    // Broadcasts the shutdown request to the background sweep so it can stop
    // cleanly before we checkpoint the WAL.
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    // Periodic background sweep of expired session rows. validate_and_touch
    // already lazily deletes stale rows it sees, but orphans (sessions never
    // revisited) need this sweep to avoid unbounded table growth.
    let sweep_handle = {
        let session_repo = session_repo.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if let Err(e) = session_repo.cleanup_expired().await {
                            tracing::warn!(error = ?e, "session cleanup_expired failed");
                        }
                    }
                    _ = shutdown_rx.changed() => break,
                }
            }
        })
    };

    let app_state = AppState {
        user_repo,
        exercise_repo,
        workout_repo,
        session_repo,
    };

    // Build router
    let app = routes::create_router(app_state);

    // Start server
    let addr = config.bind;
    tracing::info!("Starting server at http://{}", addr);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Server has stopped accepting connections and drained in-flight requests.
    // Stop the background sweep and wait for any current pass to finish before
    // we touch the DB.
    let _ = shutdown_tx.send(true);
    if let Err(e) = sweep_handle.await {
        tracing::warn!(error = ?e, "session sweep task did not stop cleanly");
    }

    // Checkpoint the WAL so the main DB file is self-contained. The pool (and
    // its connections) drops at the end of main, after which SQLite removes
    // the now-empty -wal/-shm siblings.
    if let Err(e) = db::checkpoint(&pool) {
        tracing::warn!(error = ?e, "WAL checkpoint on shutdown failed");
    }

    tracing::info!("Server shut down gracefully");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => { tracing::info!("Received Ctrl+C, shutting down..."); }
        () = terminate => { tracing::info!("Received SIGTERM, shutting down..."); }
    }
}
