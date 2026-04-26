use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "liftlog=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

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

    // Periodic background sweep of expired session rows. validate_and_touch
    // already lazily deletes stale rows it sees, but orphans (sessions never
    // revisited) need this sweep to avoid unbounded table growth.
    {
        let session_repo = session_repo.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                if let Err(e) = session_repo.cleanup_expired().await {
                    tracing::warn!(error = ?e, "session cleanup_expired failed");
                }
            }
        });
    }

    let app_state = AppState {
        user_repo,
        exercise_repo,
        workout_repo,
        session_repo,
    };

    // Build router
    let app = routes::create_router(app_state);

    // Start server
    let addr = config.server_addr();
    tracing::info!("Starting server at http://{}", addr);

    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

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
        _ = ctrl_c => { tracing::info!("Received Ctrl+C, shutting down..."); }
        _ = terminate => { tracing::info!("Received SIGTERM, shutting down..."); }
    }
}
