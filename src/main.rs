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
use rate_limit::{FailureBackoff, RateLimiter};
use repositories::{ExerciseRepository, SessionRepository, UserRepository, WorkoutRepository};
use state::AppState;
use std::sync::Arc;
use std::time::Duration;

// musl's default allocator is slow under concurrency.
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

fn build_app_state(config: &Config, pool: &db::DbPool) -> AppState {
    let mut log_salt = [0u8; 32];
    rand_core::OsRng.fill_bytes(&mut log_salt);

    AppState {
        user_repo: UserRepository::new(pool.clone()),
        exercise_repo: ExerciseRepository::new(pool.clone()),
        workout_repo: WorkoutRepository::new(pool.clone()),
        session_repo: SessionRepository::new(pool.clone()),
        login_rate_limiter: Arc::new(RateLimiter::new(5, Duration::from_secs(60))),
        login_backoff: Arc::new(FailureBackoff::for_login()),
        sensitive_action_rate_limiter: Arc::new(RateLimiter::new(5, Duration::from_secs(15 * 60))),
        trusted_proxy_header: config.trusted_proxy_header,
        trusted_proxies: Arc::new(config.trusted_proxies.clone()),
        cookie_secure: config.cookie_secure,
        hsts_max_age: config.hsts_max_age,
        hsts_include_subdomains: config.hsts_include_subdomains,
        log_salt: Arc::new(log_salt),
    }
}

/// Hourly sweep for sessions never revisited (`validate_and_touch` only
/// deletes the stale rows it sees) and expired share tokens.
fn spawn_expiry_sweep(
    state: &AppState,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    let session_repo = state.session_repo.clone();
    let workout_repo = state.workout_repo.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => sweep_expired(&session_repo, &workout_repo).await,
                _ = shutdown_rx.changed() => break,
            }
        }
    })
}

async fn sweep_expired(session_repo: &SessionRepository, workout_repo: &WorkoutRepository) {
    match session_repo.cleanup_expired().await {
        Ok(0) => {}
        Ok(n) => audit::sessions_expired_sweep(n),
        Err(e) => {
            tracing::warn!(error = ?e, "session cleanup_expired failed");
        }
    }
    // Independent of the session sweep: neither failure may skip the other.
    match workout_repo.cleanup_expired_share_tokens().await {
        Ok(0) => {}
        Ok(n) => tracing::info!(count = n, "cleared expired workout share tokens"),
        Err(e) => {
            tracing::warn!(error = ?e, "workout cleanup_expired_share_tokens failed");
        }
    }
}

fn log_proxy_trust(config: &Config) {
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
}

fn log_cookie_and_hsts(config: &Config) {
    // `warn`: liftlog can't detect HTTPS, and serving it with this off
    // silently drops `Secure` and `__Host-`. Plain-HTTP installs can ignore it.
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load `.env` first: clap's `env =`, `RUST_LOG` and `NO_COLOR` are all read
    // below, before `Config::from_env`.
    dotenvy::dotenv().ok();

    let args = Args::parse();
    init_tracing(args.log_format);

    let config = Config::from_env()?;

    tracing::info!("Connecting to database: {}", config.database_url);

    let pool = db::create_pool(&config.database_url)?;

    run_migrations(&pool)?;

    let app_state = build_app_state(&config, &pool);

    // Stops the sweep before the WAL checkpoint.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let sweep_handle = spawn_expiry_sweep(&app_state, shutdown_rx);

    log_proxy_trust(&config);

    let app = routes::create_router(app_state);

    let addr = config.bind;
    tracing::info!("Starting server at http://{}", addr);
    log_cookie_and_hsts(&config);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    // Drained; stop the sweep (waiting out a running pass) before touching the DB.
    let _ = shutdown_tx.send(true);
    if let Err(e) = sweep_handle.await {
        tracing::warn!(error = ?e, "session sweep task did not stop cleanly");
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::UserRole;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn test_pool() -> db::DbPool {
        let pool = db::create_memory_pool().expect("memory pool");
        migrations::run_migrations_for_tests(&pool).expect("migrations");
        pool
    }

    fn test_config() -> Config {
        Config {
            database_url: "sqlite::memory:".to_string(),
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            trusted_proxy_header: config::TrustedProxyHeader::XRealIp,
            trusted_proxies: vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))],
            cookie_secure: false,
            hsts_max_age: 3600,
            hsts_include_subdomains: true,
        }
    }

    #[test]
    fn build_app_state_copies_config() {
        let config = test_config();
        let state = build_app_state(&config, &test_pool());

        assert_eq!(state.trusted_proxy_header, config.trusted_proxy_header);
        assert_eq!(*state.trusted_proxies, config.trusted_proxies);
        assert!(!state.cookie_secure);
        assert_eq!(state.hsts_max_age, 3600);
        assert!(state.hsts_include_subdomains);
    }

    #[test]
    fn build_app_state_draws_a_fresh_salt_each_time() {
        let config = test_config();
        let pool = test_pool();
        let first = build_app_state(&config, &pool);
        let second = build_app_state(&config, &pool);

        assert_ne!(*first.log_salt, [0u8; 32]);
        assert_ne!(*first.log_salt, *second.log_salt);
    }

    #[test]
    fn build_app_state_limits_login_to_five_attempts() {
        let state = build_app_state(&test_config(), &test_pool());
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

        assert!((0..5).all(|_| state.login_rate_limiter.try_acquire(ip)));
        assert!(!state.login_rate_limiter.try_acquire(ip));
    }

    #[test]
    fn build_app_state_limits_sensitive_actions_to_five_attempts() {
        let state = build_app_state(&test_config(), &test_pool());
        let user_id = "user".to_string();

        assert!((0..5).all(|_| {
            state
                .sensitive_action_rate_limiter
                .try_acquire(user_id.clone())
        }));
        assert!(
            !state
                .sensitive_action_rate_limiter
                .try_acquire(user_id.clone())
        );
    }

    fn count_rows(pool: &db::DbPool, sql: &str) -> i64 {
        pool.get()
            .unwrap()
            .query_row(sql, [], |row| row.get(0))
            .unwrap()
    }

    #[tokio::test]
    async fn sweep_expired_removes_expired_sessions_and_share_tokens() {
        let pool = test_pool();
        let state = build_app_state(&test_config(), &pool);
        let user = state
            .user_repo
            .create("sweeper", "password", UserRole::User)
            .await
            .unwrap();

        state.session_repo.create(&user.id).await.unwrap();
        let expired_token = state.session_repo.create(&user.id).await.unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
                [&expired_token],
            )
            .unwrap();

        let today = chrono::Utc::now().date_naive();
        let live_workout = state
            .workout_repo
            .create_session(&user.id, today, None)
            .await
            .unwrap();
        let expired_workout = state
            .workout_repo
            .create_session(&user.id, today, None)
            .await
            .unwrap();
        state
            .workout_repo
            .set_share_token(&live_workout.id, &user.id, Some(chrono::Duration::hours(1)))
            .await
            .unwrap();
        state
            .workout_repo
            .set_share_token(
                &expired_workout.id,
                &user.id,
                Some(chrono::Duration::hours(-1)),
            )
            .await
            .unwrap();

        sweep_expired(&state.session_repo, &state.workout_repo).await;

        assert_eq!(count_rows(&pool, "SELECT COUNT(*) FROM sessions"), 1);
        assert_eq!(
            count_rows(
                &pool,
                "SELECT COUNT(*) FROM workout_sessions WHERE share_token IS NOT NULL"
            ),
            1
        );
    }
}
