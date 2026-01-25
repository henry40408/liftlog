use std::path::PathBuf;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod db;
mod error;
mod handlers;
mod middleware;
mod models;
mod repositories;
mod routes;
mod session;

use config::Config;
use db::DbPool;
use handlers::{auth, dashboard, exercises, stats, workouts};
use repositories::{ExerciseRepository, UserRepository, WorkoutRepository};
use session::SessionKey;

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

    // Generate session key
    let session_key = SessionKey::generate();

    // Create repositories
    let user_repo = UserRepository::new(pool.clone());
    let exercise_repo = ExerciseRepository::new(pool.clone());
    let workout_repo = WorkoutRepository::new(pool.clone());

    // Create handler states
    let auth_state = auth::AuthState {
        user_repo: user_repo.clone(),
    };
    let dashboard_state = dashboard::DashboardState {
        workout_repo: workout_repo.clone(),
    };
    let workouts_state = workouts::WorkoutsState {
        workout_repo: workout_repo.clone(),
        exercise_repo: exercise_repo.clone(),
    };
    let exercises_state = exercises::ExercisesState {
        exercise_repo: exercise_repo.clone(),
    };
    let stats_state = stats::StatsState {
        workout_repo: workout_repo.clone(),
        exercise_repo: exercise_repo.clone(),
    };

    // Build router
    let app = routes::create_router(
        auth_state,
        dashboard_state,
        workouts_state,
        exercises_state,
        stats_state,
        session_key,
    );

    // Start server
    let addr = config.server_addr();
    tracing::info!("Starting server at http://{}", addr);

    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn run_migrations(pool: &DbPool) -> anyhow::Result<()> {
    tracing::info!("Running migrations...");

    let conn = pool.get()?;

    let migrations_dir = PathBuf::from("migrations");
    let mut entries: Vec<_> = std::fs::read_dir(&migrations_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "sql").unwrap_or(false))
        .collect();

    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let filename = path.file_name().unwrap().to_string_lossy();
        tracing::info!("Running migration: {}", filename);

        let sql = std::fs::read_to_string(&path)?;
        conn.execute_batch(&sql)?;
    }

    tracing::info!("Migrations completed");
    Ok(())
}
