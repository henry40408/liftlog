use axum::Router;

use liftlog::db::{DbPool, create_memory_pool};
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

/// A password that satisfies the policy in `models::user::password_policy_error`
/// (length floor plus a `zxcvbn` score of at least 3). Used by every test whose
/// subject is something *other* than password strength, so tightening the
/// policy does not mean hunting through the suite for newly-invalid literals.
///
/// Contains no characters that form-encoding would alter, so it can be
/// interpolated straight into a request body.
#[allow(dead_code)]
pub const STRONG_PASSWORD: &str = "purple-monkey-dishwasher";

/// Budget handed to whichever rate limiter a given helper is not there to
/// exercise, so a test targeting one throttle can never be tripped by the
/// other.
const GENEROUS_MAX_ATTEMPTS: u32 = 100;
const GENEROUS_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);

#[allow(dead_code)]
pub fn create_test_app(pool: DbPool) -> Router {
    create_test_app_with_session(pool).router
}

pub fn create_test_app_with_session(pool: DbPool) -> TestApp {
    // A generous default so existing tests (which don't exercise rate
    // limiting) can't trip it.
    create_test_app_with_rate_limit(pool, 100, std::time::Duration::from_secs(60))
}

/// Builds an app whose *password-change* throttle is tightened to
/// `max_attempts` per `window`, leaving the login throttle generous. The two
/// limiters are configured independently because they key on different
/// things (client IP vs user id) and a test exercising one must not be able
/// to trip the other by accident.
#[allow(dead_code)]
pub fn create_test_app_with_password_change_limit(
    pool: DbPool,
    max_attempts: u32,
    window: std::time::Duration,
) -> TestApp {
    build_test_app(
        pool,
        GENEROUS_MAX_ATTEMPTS,
        GENEROUS_WINDOW,
        max_attempts,
        window,
        false,
        liftlog::config::TrustedProxyHeader::None,
        Vec::new(),
        0,
        false,
    )
}

#[allow(dead_code)]
pub fn create_test_app_with_rate_limit(
    pool: DbPool,
    max_attempts: u32,
    window: std::time::Duration,
) -> TestApp {
    build_test_app(
        pool,
        max_attempts,
        window,
        GENEROUS_MAX_ATTEMPTS,
        GENEROUS_WINDOW,
        false,
        liftlog::config::TrustedProxyHeader::None,
        Vec::new(),
        0,
        false,
    )
}

#[allow(dead_code)]
pub fn create_test_app_with_cookie_secure(pool: DbPool, cookie_secure: bool) -> TestApp {
    build_test_app(
        pool,
        100,
        std::time::Duration::from_secs(60),
        GENEROUS_MAX_ATTEMPTS,
        GENEROUS_WINDOW,
        cookie_secure,
        liftlog::config::TrustedProxyHeader::None,
        Vec::new(),
        0,
        false,
    )
}

/// Like [`create_test_app_with_rate_limit`], but also selects which
/// forwarding header (if any) is trusted, and which peers may supply it —
/// for tests exercising `crate::net::client_ip` end to end through the
/// router.
#[allow(dead_code)]
pub fn create_test_app_with_proxy_header(
    pool: DbPool,
    max_attempts: u32,
    window: std::time::Duration,
    header: liftlog::config::TrustedProxyHeader,
    trusted_proxies: Vec<std::net::IpAddr>,
) -> TestApp {
    build_test_app(
        pool,
        max_attempts,
        window,
        GENEROUS_MAX_ATTEMPTS,
        GENEROUS_WINDOW,
        false,
        header,
        trusted_proxies,
        0,
        false,
    )
}

/// Like [`create_test_app_with_rate_limit`], but also configures the HSTS
/// header — for tests exercising `middleware::security_headers` end to end
/// through the router.
#[allow(dead_code)]
pub fn create_test_app_with_hsts(pool: DbPool, max_age: u64, include_subdomains: bool) -> TestApp {
    build_test_app(
        pool,
        100,
        std::time::Duration::from_secs(60),
        GENEROUS_MAX_ATTEMPTS,
        GENEROUS_WINDOW,
        false,
        liftlog::config::TrustedProxyHeader::None,
        Vec::new(),
        max_age,
        include_subdomains,
    )
}

/// Single place that actually builds `AppState`; every other
/// `create_test_app_*` helper delegates here.
#[allow(clippy::too_many_arguments)]
fn build_test_app(
    pool: DbPool,
    max_attempts: u32,
    window: std::time::Duration,
    password_change_max_attempts: u32,
    password_change_window: std::time::Duration,
    cookie_secure: bool,
    trusted_proxy_header: liftlog::config::TrustedProxyHeader,
    trusted_proxies: Vec<std::net::IpAddr>,
    hsts_max_age: u64,
    hsts_include_subdomains: bool,
) -> TestApp {
    use liftlog::rate_limit::RateLimiter;
    use liftlog::repositories::{ExerciseRepository, WorkoutRepository};
    use liftlog::state::AppState;
    use std::sync::Arc;

    let app_state = AppState {
        user_repo: UserRepository::new(pool.clone()),
        exercise_repo: ExerciseRepository::new(pool.clone()),
        workout_repo: WorkoutRepository::new(pool.clone()),
        session_repo: SessionRepository::new(pool.clone()),
        login_rate_limiter: Arc::new(RateLimiter::new(max_attempts, window)),
        sensitive_action_rate_limiter: Arc::new(RateLimiter::new(
            password_change_max_attempts,
            password_change_window,
        )),
        trusted_proxy_header,
        trusted_proxies: Arc::new(trusted_proxies),
        cookie_secure,
        // Disabled by default so every existing test keeps observing
        // current (no-HSTS) behaviour; only `create_test_app_with_hsts`
        // opts in.
        hsts_max_age,
        hsts_include_subdomains,
        // Fixed, deterministic salt (not random) so a test can assert a
        // specific fingerprint if it ever needs to; nothing in this test
        // suite currently relies on its exact value.
        log_salt: Arc::new([7u8; 32]),
    };

    let router = liftlog::routes::create_router(app_state);

    TestApp { router }
}

// Shared test helper used by a subset of integration test binaries.
#[allow(dead_code)]
pub async fn create_test_user(
    pool: &DbPool,
    username: &str,
    password: &str,
    role: UserRole,
) -> User {
    let user_repo = UserRepository::new(pool.clone());
    user_repo.create(username, password, role).await.unwrap()
}

#[allow(dead_code)]
pub async fn create_session_token(pool: &DbPool, user: &User) -> String {
    let session_repo = SessionRepository::new(pool.clone());
    session_repo.create(&user.id).await.unwrap()
}

#[allow(dead_code)]
pub fn cookie_header(token: &str) -> String {
    format!("{}={token}", liftlog::session::session_cookie_name(false))
}

#[allow(dead_code)]
pub fn cookie_header_secure(token: &str) -> String {
    format!("{}={token}", liftlog::session::session_cookie_name(true))
}

#[allow(dead_code)]
pub async fn create_session_cookie(pool: &DbPool, user: &User) -> String {
    cookie_header(&create_session_token(pool, user).await)
}

/// Attach a `ConnectInfo` extension so `login_submit` sees a TCP peer, the way
/// `into_make_service_with_connect_info` does in production. `oneshot` does not
/// go through that layer.
#[allow(dead_code)]
pub fn with_peer(
    mut request: axum::http::Request<axum::body::Body>,
    peer: &str,
) -> axum::http::Request<axum::body::Body> {
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        peer.parse::<std::net::SocketAddr>()
            .expect("valid peer socket address"),
    ));
    request
}

#[allow(dead_code)]
pub fn extract_cookie_header(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap_or("").to_string()
}

#[allow(dead_code)]
pub fn age_session_touch(pool: &DbPool, token: &str, hours_ago: u32) {
    let conn = pool.get().unwrap();
    let sql = format!(
        "UPDATE sessions SET last_touched_at = datetime('now', '-{hours_ago} hours') WHERE token = ?"
    );
    conn.execute(&sql, [token]).unwrap();
}

#[allow(dead_code)]
pub fn expire_session(pool: &DbPool, token: &str) {
    let conn = pool.get().unwrap();
    conn.execute(
        "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
        [token],
    )
    .unwrap();
}

#[allow(dead_code)]
pub fn age_session_creation(pool: &DbPool, token: &str, days_ago: u32) {
    let conn = pool.get().unwrap();
    let sql = format!(
        "UPDATE sessions SET created_at = datetime('now', '-{days_ago} days') WHERE token = ?"
    );
    conn.execute(&sql, [token]).unwrap();
}

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
