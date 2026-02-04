use axum::Router;

use liftlog::db::{create_memory_pool, DbPool};
use liftlog::migrations::run_migrations_for_tests;
use liftlog::models::{User, UserRole};
use liftlog::repositories::{SessionRepository, UserRepository};

pub fn setup_test_db() -> DbPool {
    let pool = create_memory_pool().expect("Failed to create test database");
    run_migrations_for_tests(&pool).expect("Failed to run migrations");
    pool
}

pub struct TestApp {
    pub router: Router,
    pub session_repo: SessionRepository,
}

pub fn create_test_app(pool: DbPool) -> Router {
    create_test_app_with_session(pool).router
}

pub fn create_test_app_with_session(pool: DbPool) -> TestApp {
    use liftlog::handlers::{auth, dashboard, exercises, settings, stats, workouts};
    use liftlog::repositories::{ExerciseRepository, WorkoutRepository};

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

    let router = liftlog::routes::create_router(
        auth_state,
        dashboard_state,
        workouts_state,
        exercises_state,
        stats_state,
        settings_state,
    );

    TestApp {
        router,
        session_repo,
    }
}

pub async fn create_test_user(
    pool: &DbPool,
    username: &str,
    password: &str,
    role: UserRole,
) -> User {
    let user_repo = UserRepository::new(pool.clone());
    user_repo.create(username, password, role).await.unwrap()
}

pub async fn create_session_cookie(pool: &DbPool, user: &User) -> String {
    let session_repo = SessionRepository::new(pool.clone());
    let token = session_repo.create(&user.id).await.unwrap();
    format!("session={}", token)
}

pub fn extract_cookie_header(set_cookie: &str) -> String {
    // Extract just the cookie name=value part for use in Cookie header
    set_cookie.split(';').next().unwrap_or("").to_string()
}

// Test data creation helpers
pub async fn create_test_exercise(
    pool: &DbPool,
    user_id: &str,
    name: &str,
    category: &str,
) -> liftlog::models::Exercise {
    let exercise_repo = liftlog::repositories::ExerciseRepository::new(pool.clone());
    exercise_repo.create(name, category, user_id).await.unwrap()
}

pub async fn create_test_workout(
    pool: &DbPool,
    user_id: &str,
    date: chrono::NaiveDate,
    notes: Option<&str>,
) -> liftlog::models::WorkoutSession {
    let workout_repo = liftlog::repositories::WorkoutRepository::new(pool.clone());
    workout_repo
        .create_session(user_id, date, notes)
        .await
        .unwrap()
}

pub async fn create_test_log(
    pool: &DbPool,
    session_id: &str,
    exercise_id: &str,
    set_number: i32,
    reps: i32,
    weight: f64,
    rpe: Option<i32>,
) -> liftlog::models::WorkoutLog {
    let workout_repo = liftlog::repositories::WorkoutRepository::new(pool.clone());
    workout_repo
        .create_log(session_id, exercise_id, set_number, reps, weight, rpe)
        .await
        .unwrap()
}
