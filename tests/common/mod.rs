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
}

#[allow(dead_code)]
pub fn create_test_app(pool: DbPool) -> Router {
    create_test_app_with_session(pool).router
}

pub fn create_test_app_with_session(pool: DbPool) -> TestApp {
    use liftlog::repositories::{ExerciseRepository, WorkoutRepository};
    use liftlog::state::AppState;

    let app_state = AppState {
        user_repo: UserRepository::new(pool.clone()),
        exercise_repo: ExerciseRepository::new(pool.clone()),
        workout_repo: WorkoutRepository::new(pool.clone()),
        session_repo: SessionRepository::new(pool.clone()),
    };

    let router = liftlog::routes::create_router(app_state);

    TestApp { router }
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

pub async fn create_session_token(pool: &DbPool, user: &User) -> String {
    let session_repo = SessionRepository::new(pool.clone());
    session_repo.create(&user.id).await.unwrap()
}

pub fn cookie_header(token: &str) -> String {
    format!("session={}", token)
}

pub async fn create_session_cookie(pool: &DbPool, user: &User) -> String {
    cookie_header(&create_session_token(pool, user).await)
}

pub fn extract_cookie_header(set_cookie: &str) -> String {
    // Extract just the cookie name=value part for use in Cookie header
    set_cookie.split(';').next().unwrap_or("").to_string()
}

#[allow(dead_code)]
pub async fn age_session_touch(pool: &DbPool, token: &str, hours_ago: u32) {
    let conn = pool.get().unwrap();
    let sql = format!(
        "UPDATE sessions SET last_touched_at = datetime('now', '-{} hours') WHERE token = ?",
        hours_ago
    );
    conn.execute(&sql, [token]).unwrap();
}

#[allow(dead_code)]
pub async fn expire_session(pool: &DbPool, token: &str) {
    let conn = pool.get().unwrap();
    conn.execute(
        "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
        [token],
    )
    .unwrap();
}

// Test data creation helpers
#[allow(dead_code)]
pub async fn create_test_exercise(
    pool: &DbPool,
    user_id: &str,
    name: &str,
    category: &str,
) -> liftlog::models::Exercise {
    let exercise_repo = liftlog::repositories::ExerciseRepository::new(pool.clone());
    exercise_repo.create(name, category, user_id).await.unwrap()
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
