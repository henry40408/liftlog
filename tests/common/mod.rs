use axum::Router;
use std::path::PathBuf;

use liftlog::db::{create_memory_pool, DbPool};
use liftlog::models::{User, UserRole};
use liftlog::repositories::UserRepository;
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
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "sql")
                .unwrap_or(false)
        })
        .collect();

    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let sql = std::fs::read_to_string(&path)?;
        conn.execute_batch(&sql)?;
    }

    Ok(())
}

pub struct TestApp {
    pub router: Router,
    pub session_key: SessionKey,
}

pub fn create_test_app(pool: DbPool) -> Router {
    create_test_app_with_key(pool).router
}

pub fn create_test_app_with_key(pool: DbPool) -> TestApp {
    use liftlog::handlers::{auth, dashboard, exercises, stats, workouts};
    use liftlog::repositories::{ExerciseRepository, WorkoutRepository};

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

    let router = liftlog::routes::create_router(
        auth_state,
        dashboard_state,
        workouts_state,
        exercises_state,
        stats_state,
        session_key.clone(),
    );

    TestApp {
        router,
        session_key,
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

pub fn create_session_cookie(user: &User, session_key: &SessionKey) -> String {
    use axum::http::HeaderMap;
    use axum_extra::extract::cookie::SignedCookieJar;
    use liftlog::middleware::AuthUser;

    let jar = SignedCookieJar::from_headers(&HeaderMap::new(), session_key.0.clone());
    let jar = AuthUser::login(jar, user);

    // Extract the cookie from the jar using into_response
    use axum::response::IntoResponse;
    let response = jar.into_response();
    let headers = response.headers();

    headers
        .get(axum::http::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

pub fn extract_cookie_header(set_cookie: &str) -> String {
    // Extract just the cookie name=value part for use in Cookie header
    set_cookie.split(';').next().unwrap_or("").to_string()
}
