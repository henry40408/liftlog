use axum::{
    routing::{get, post},
    Router,
};
use tower_http::services::ServeDir;

use crate::handlers::{auth, dashboard, exercises, stats, workouts};

pub fn create_router(
    auth_state: auth::AuthState,
    dashboard_state: dashboard::DashboardState,
    workouts_state: workouts::WorkoutsState,
    exercises_state: exercises::ExercisesState,
    stats_state: stats::StatsState,
) -> Router {
    Router::new()
        // Dashboard
        .route("/", get(dashboard::index))
        .with_state(dashboard_state)
        // Auth routes
        .route("/auth/login", get(auth::login_page).post(auth::login_submit))
        .route("/auth/setup", get(auth::setup_page).post(auth::setup_submit))
        .route("/auth/logout", post(auth::logout))
        .route("/users", get(auth::users_list))
        .route("/users/new", get(auth::new_user_page).post(auth::new_user_submit))
        .with_state(auth_state)
        // Workout routes
        .route("/workouts", get(workouts::list))
        .route("/workouts/new", get(workouts::new_page))
        .route("/workouts", post(workouts::create))
        .route("/workouts/:id", get(workouts::show))
        .route("/workouts/:id/edit", get(workouts::edit_page))
        .route("/workouts/:id", post(workouts::update))
        .route("/workouts/:id/delete", post(workouts::delete))
        .route("/workouts/:id/logs", post(workouts::add_log))
        .route("/workouts/:id/logs/:log_id/delete", post(workouts::delete_log))
        .with_state(workouts_state)
        // Exercise routes
        .route("/exercises", get(exercises::list))
        .route("/exercises/new", get(exercises::new_page))
        .route("/exercises", post(exercises::create))
        .with_state(exercises_state)
        // Stats routes
        .route("/stats", get(stats::index))
        .route("/stats/exercise/:id", get(stats::exercise_stats))
        .route("/stats/prs", get(stats::prs_list))
        .with_state(stats_state)
        // Static files
        .nest_service("/static", ServeDir::new("static"))
}
