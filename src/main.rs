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
mod version;

use config::Config;
use handlers::{auth, dashboard, exercises, settings, stats, workouts};
use migrations::run_migrations;
use repositories::{ExerciseRepository, SessionRepository, UserRepository, WorkoutRepository};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "liftlog=debug,tower_http=debug".into()),
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

    // Create handler states
    let auth_state = auth::AuthState {
        user_repo: user_repo.clone(),
        session_repo: session_repo.clone(),
    };
    let dashboard_state = dashboard::DashboardState {
        workout_repo: workout_repo.clone(),
    };
    let workouts_state = workouts::WorkoutsState {
        workout_repo: workout_repo.clone(),
        exercise_repo: exercise_repo.clone(),
        user_repo: user_repo.clone(),
    };
    let exercises_state = exercises::ExercisesState {
        exercise_repo: exercise_repo.clone(),
    };
    let stats_state = stats::StatsState {
        workout_repo: workout_repo.clone(),
        exercise_repo: exercise_repo.clone(),
    };
    let settings_state = settings::SettingsState {
        user_repo: user_repo.clone(),
        session_repo: session_repo.clone(),
    };

    // Build router
    let app = routes::create_router(
        auth_state,
        dashboard_state,
        workouts_state,
        exercises_state,
        stats_state,
        settings_state,
    );

    // Start server
    let addr = config.server_addr();
    tracing::info!("Starting server at http://{}", addr);

    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
