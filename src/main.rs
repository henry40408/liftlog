use clap::Parser;
use tokio::net::TcpListener;
use tracing_subscriber::{
    EnvFilter, Layer as _, fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt,
};

mod audit;
mod config;
mod db;
mod error;
mod handlers;
mod middleware;
mod migrations;
mod models;
mod net;
mod rate_limit;
mod repositories;
mod routes;
mod session;
mod state;
mod version;

use config::Config;
use migrations::run_migrations;
use rand_core::RngCore;
use rate_limit::RateLimiter;
use repositories::{ExerciseRepository, SessionRepository, UserRepository, WorkoutRepository};
use state::AppState;
use std::sync::Arc;
use std::time::Duration;

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
    #[arg(long, env = "LIFTLOG_LOG_FORMAT", default_value = "full")]
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

    dotenvy::dotenv().ok();

    let config = Config::from_env()?;

    tracing::info!("Connecting to database: {}", config.database_url);

    let pool = db::create_pool(&config.database_url)?;

    run_migrations(&pool)?;

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
        // Cloned here (not moved) because `workout_repo` is also captured by
        // value in `app_state` below.
        let workout_repo = workout_repo.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        match session_repo.cleanup_expired().await {
                            // Only when something was actually retired: an
                            // idle deployment would otherwise emit one empty
                            // line an hour, forever.
                            Ok(0) => {}
                            Ok(n) => audit::sessions_expired_sweep(n),
                            Err(e) => {
                                tracing::warn!(error = ?e, "session cleanup_expired failed");
                            }
                        }
                        // A separate match, not `?` or an early return: a
                        // failure clearing dead share tokens must not skip
                        // (or be skipped by) the session sweep above — the
                        // two are unrelated lifecycles sharing one ticker.
                        match workout_repo.cleanup_expired_share_tokens().await {
                            Ok(0) => {}
                            // Not a session lifecycle event, so this stays
                            // off the `liftlog::audit` target and is just a
                            // plain info log.
                            Ok(n) => tracing::info!(count = n, "cleared expired workout share tokens"),
                            Err(e) => {
                                tracing::warn!(error = ?e, "workout cleanup_expired_share_tokens failed");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => break,
                }
            }
        })
    };

    match config.trusted_proxy_header {
        config::TrustedProxyHeader::None => {
            tracing::warn!(
                "LIFTLOG_TRUSTED_PROXY_HEADER is not set; forwarding headers are ignored and the TCP peer is used for login rate limiting — behind a reverse proxy every client shares one bucket. Set it to x-forwarded-for or x-real-ip once your proxy is configured to overwrite that header."
            );
        }
        header if config.trusted_proxies.is_empty() => {
            tracing::info!(
                ?header,
                "trusting forwarding header from loopback peers only; set LIFTLOG_TRUSTED_PROXIES if the proxy runs on another host or in another container"
            );
        }
        header => {
            tracing::info!(
                ?header,
                proxies = ?config.trusted_proxies,
                "trusting forwarding header from configured proxies"
            );
        }
    }

    // Per-process salt for audit-log session fingerprints. Generated fresh
    // on every startup — never logged, never persisted — so a leaked log
    // line can never be used to recover or replay a session token.
    let mut log_salt = [0u8; 32];
    rand_core::OsRng.fill_bytes(&mut log_salt);

    let app_state = AppState {
        user_repo,
        exercise_repo,
        workout_repo,
        session_repo,
        login_rate_limiter: Arc::new(RateLimiter::new(5, Duration::from_secs(60))),
        sensitive_action_rate_limiter: Arc::new(RateLimiter::new(5, Duration::from_secs(15 * 60))),
        trusted_proxy_header: config.trusted_proxy_header,
        trusted_proxies: Arc::new(config.trusted_proxies.clone()),
        cookie_secure: config.cookie_secure,
        hsts_max_age: config.hsts_max_age,
        hsts_include_subdomains: config.hsts_include_subdomains,
        log_salt: Arc::new(log_salt),
    };

    // Build router
    let app = routes::create_router(app_state);

    // Start server
    let addr = config.bind;
    tracing::info!("Starting server at http://{}", addr);
    // Not an `info!` like the rest of this block. liftlog never terminates
    // TLS, so it cannot detect that it is being served over HTTPS with this
    // left off — and that combination silently drops both the `Secure`
    // attribute and the `__Host-` cookie prefix, which is precisely the
    // misconfiguration nobody notices because everything still works. A
    // deployment that really is plain HTTP (loopback, a private LAN) will see
    // this warning too and can ignore it; the asymmetry is deliberate, since
    // one case is a security hole and the other is a line of log noise.
    if config.cookie_secure {
        tracing::info!("session cookie Secure attribute is enabled");
    } else {
        tracing::warn!(
            "LIFTLOG_COOKIE_SECURE is off: the session cookie is sent without `Secure` and without the `__Host-` prefix, so it will travel over plain HTTP and can be overwritten by a sibling subdomain. Set LIFTLOG_COOKIE_SECURE=true if this deployment is served over HTTPS."
        );
    }
    if config.hsts_max_age > 0 {
        tracing::info!(
            max_age = config.hsts_max_age,
            include_subdomains = config.hsts_include_subdomains,
            "HSTS enabled"
        );
    }

    let listener = TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
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
