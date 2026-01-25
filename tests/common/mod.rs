use axum::Router;
use std::path::PathBuf;

use liftlog::db::{create_memory_pool, DbPool};
use liftlog::session::SessionKey;

pub fn setup_test_db() -> DbPool {
    let pool = create_memory_pool().expect("Failed to create test database");

    // Run migrations
    run_migrations(&pool).expect("Failed to run migrations");

    pool
}

fn run_migrations(pool: &DbPool) -> Result<(), Box<dyn std::error::Error>> {
    let conn = pool.get()?;

    let migrations_dir = PathBuf::from("migrations");
    let mut entries: Vec<_> = std::fs::read_dir(&migrations_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "sql").unwrap_or(false))
        .collect();

    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let sql = std::fs::read_to_string(&path)?;
        conn.execute_batch(&sql)?;
    }

    Ok(())
}

pub fn create_test_app(pool: DbPool) -> Router {
    use liftlog::handlers::{auth, dashboard, exercises, stats, workouts};
    use liftlog::repositories::{ExerciseRepository, UserRepository, WorkoutRepository};

    // Generate session key for tests
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

    liftlog::routes::create_router(
        auth_state,
        dashboard_state,
        workouts_state,
        exercises_state,
        stats_state,
        session_key,
    )
}
