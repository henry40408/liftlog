use axum::Router;

use liftlog::db::{DbPool, create_memory_pool};
use liftlog::migrations::run_migrations_for_tests;
use liftlog::models::{User, UserRole};
use liftlog::repositories::{SessionRepository, UserRepository};

#[allow(dead_code)]
pub fn setup_test_db() -> DbPool {
    let pool = create_memory_pool().expect("Failed to create test database");
    run_migrations_for_tests(&pool).expect("Failed to run migrations");
    pool
}

pub struct TestApp {
    pub router: Router,
    #[allow(dead_code)]
    pub setup_token: std::sync::Arc<liftlog::setup_token::SetupToken>,
}

/// Passes `password_policy_error`, for tests not about password strength.
/// Form-encoding-safe.
#[allow(dead_code)]
pub const STRONG_PASSWORD: &str = "purple-monkey-dishwasher";

/// The setup token every test app starts with; there is deliberately no way to
/// set it in production.
#[allow(dead_code)]
pub const TEST_SETUP_TOKEN: &str = "ABCDE-FGHJK";

/// Budget for whichever limiter a helper is not exercising.
const GENEROUS_MAX_ATTEMPTS: u32 = 100;
const GENEROUS_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);

/// Backoff for tests not about it: the counter runs, but a zero base never sleeps.
const NO_BACKOFF_FREE_ATTEMPTS: u32 = 3;
const NO_BACKOFF_BASE: std::time::Duration = std::time::Duration::ZERO;

/// Settings for [`build_test_app`]. The default leaves every throttle generous,
/// the backoff inert, and cookies, proxy header and HSTS off, so a helper
/// lists only the fields it exercises.
struct TestAppOptions {
    login_max_attempts: u32,
    login_window: std::time::Duration,
    password_change_max_attempts: u32,
    password_change_window: std::time::Duration,
    backoff_free_attempts: u32,
    backoff_base: std::time::Duration,
    cookie_secure: bool,
    trusted_proxy_header: liftlog::config::TrustedProxyHeader,
    trusted_proxies: Vec<std::net::IpAddr>,
    hsts_max_age: u64,
    hsts_include_subdomains: bool,
    setup_max_attempts: u32,
}

impl Default for TestAppOptions {
    fn default() -> Self {
        Self {
            login_max_attempts: GENEROUS_MAX_ATTEMPTS,
            login_window: GENEROUS_WINDOW,
            password_change_max_attempts: GENEROUS_MAX_ATTEMPTS,
            password_change_window: GENEROUS_WINDOW,
            backoff_free_attempts: NO_BACKOFF_FREE_ATTEMPTS,
            backoff_base: NO_BACKOFF_BASE,
            cookie_secure: false,
            trusted_proxy_header: liftlog::config::TrustedProxyHeader::None,
            trusted_proxies: Vec::new(),
            hsts_max_age: 0,
            hsts_include_subdomains: false,
            setup_max_attempts: GENEROUS_MAX_ATTEMPTS,
        }
    }
}

#[allow(dead_code)]
pub fn create_test_app(pool: DbPool) -> Router {
    create_test_app_with_session(pool).router
}

pub fn create_test_app_with_session(pool: DbPool) -> TestApp {
    build_test_app(pool, TestAppOptions::default())
}

/// Tightens only the password-change throttle.
#[allow(dead_code)]
pub fn create_test_app_with_password_change_limit(
    pool: DbPool,
    max_attempts: u32,
    window: std::time::Duration,
) -> TestApp {
    build_test_app(
        pool,
        TestAppOptions {
            password_change_max_attempts: max_attempts,
            password_change_window: window,
            ..Default::default()
        },
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
        TestAppOptions {
            login_max_attempts: max_attempts,
            login_window: window,
            ..Default::default()
        },
    )
}

#[allow(dead_code)]
pub fn create_test_app_with_cookie_secure(pool: DbPool, cookie_secure: bool) -> TestApp {
    build_test_app(
        pool,
        TestAppOptions {
            cookie_secure,
            ..Default::default()
        },
    )
}

/// Like [`create_test_app_with_rate_limit`], plus the trusted forwarding
/// header and the peers allowed to send it.
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
        TestAppOptions {
            login_max_attempts: max_attempts,
            login_window: window,
            trusted_proxy_header: header,
            trusted_proxies,
            ..Default::default()
        },
    )
}

/// Like [`create_test_app_with_rate_limit`], plus HSTS.
#[allow(dead_code)]
pub fn create_test_app_with_hsts(pool: DbPool, max_age: u64, include_subdomains: bool) -> TestApp {
    build_test_app(
        pool,
        TestAppOptions {
            hsts_max_age: max_age,
            hsts_include_subdomains: include_subdomains,
            ..Default::default()
        },
    )
}

/// Tightens only the setup throttle.
#[allow(dead_code)]
pub fn create_test_app_with_setup_limit(pool: DbPool, max_attempts: u32) -> TestApp {
    build_test_app(
        pool,
        TestAppOptions {
            setup_max_attempts: max_attempts,
            ..Default::default()
        },
    )
}

/// Enables the per-account login backoff.
#[allow(dead_code)]
pub fn create_test_app_with_login_backoff(
    pool: DbPool,
    free_attempts: u32,
    base: std::time::Duration,
) -> TestApp {
    build_test_app(
        pool,
        TestAppOptions {
            backoff_free_attempts: free_attempts,
            backoff_base: base,
            ..Default::default()
        },
    )
}

fn build_test_app(pool: DbPool, options: TestAppOptions) -> TestApp {
    use liftlog::rate_limit::{FailureBackoff, RateLimiter};
    use liftlog::repositories::{ExerciseRepository, WorkoutRepository};
    use liftlog::state::AppState;
    use std::sync::Arc;

    let setup_token = Arc::new(liftlog::setup_token::SetupToken::with_value(
        TEST_SETUP_TOKEN,
    ));
    let app_state = AppState {
        user_repo: UserRepository::new(pool.clone()),
        exercise_repo: ExerciseRepository::new(pool.clone()),
        workout_repo: WorkoutRepository::new(pool.clone()),
        session_repo: SessionRepository::new(pool.clone()),
        login_rate_limiter: Arc::new(RateLimiter::new(
            options.login_max_attempts,
            options.login_window,
        )),
        setup_rate_limiter: Arc::new(RateLimiter::new(
            options.setup_max_attempts,
            GENEROUS_WINDOW,
        )),
        setup_token: setup_token.clone(),
        login_backoff: Arc::new(FailureBackoff::new(
            options.backoff_free_attempts,
            options.backoff_base,
            // Cap at 4x the base so no test waits long.
            options.backoff_base * 4,
            std::time::Duration::from_secs(60),
        )),
        sensitive_action_rate_limiter: Arc::new(RateLimiter::new(
            options.password_change_max_attempts,
            options.password_change_window,
        )),
        trusted_proxy_header: options.trusted_proxy_header,
        trusted_proxies: Arc::new(options.trusted_proxies),
        cookie_secure: options.cookie_secure,
        hsts_max_age: options.hsts_max_age,
        hsts_include_subdomains: options.hsts_include_subdomains,
        // Fixed salt, so fingerprints are deterministic.
        log_salt: Arc::new([7u8; 32]),
    };

    let router = liftlog::routes::create_router(app_state);

    TestApp {
        router,
        setup_token,
    }
}

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

/// Attaches the `ConnectInfo` that `oneshot` skips, so handlers see a peer.
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
    new_log: liftlog::repositories::NewWorkoutLog<'_>,
) -> liftlog::models::WorkoutLog {
    let workout_repo = liftlog::repositories::WorkoutRepository::new(pool.clone());
    workout_repo.create_log(new_log).await.unwrap()
}

/// File-backed DB with the production pool (size 10), unlike the
/// `max_size(1)` memory pool, so concurrent requests really overlap.
/// Deletes the DB file (and WAL/SHM siblings) on drop.
#[allow(dead_code)]
pub struct FileTestDb {
    pub pool: DbPool,
    path: std::path::PathBuf,
}

#[allow(dead_code)]
impl FileTestDb {
    pub fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("liftlog-it-{}.sqlite3", uuid::Uuid::new_v4()));
        let pool = liftlog::db::create_pool(path.to_str().unwrap()).expect("file pool");
        run_migrations_for_tests(&pool).expect("Failed to run migrations");
        Self { pool, path }
    }
}

impl Drop for FileTestDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(format!("{}-wal", self.path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.path.display()));
    }
}
