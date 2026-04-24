use axum::{
    middleware::from_fn_with_state,
    routing::{get, post},
    Extension, Router,
};

use crate::handlers::{auth, dashboard, exercises, favicon, health, settings, stats, workouts};
use crate::middleware::sliding_session_middleware;

pub fn create_router(
    auth_state: auth::AuthState,
    dashboard_state: dashboard::DashboardState,
    workouts_state: workouts::WorkoutsState,
    exercises_state: exercises::ExercisesState,
    stats_state: stats::StatsState,
    settings_state: settings::SettingsState,
) -> Router {
    let session_repo = auth_state.session_repo.clone();
    let user_repo = auth_state.user_repo.clone();

    Router::new()
        // Health check
        .route("/health", get(health::health_check))
        // Favicon (no auth, no state)
        .route("/favicon.svg", get(favicon::favicon_svg))
        .route("/apple-touch-icon.png", get(favicon::apple_touch_icon))
        // Dashboard
        .route("/", get(dashboard::index))
        .with_state(dashboard_state)
        // Auth routes
        .route(
            "/auth/login",
            get(auth::login_page).post(auth::login_submit),
        )
        .route(
            "/auth/setup",
            get(auth::setup_page).post(auth::setup_submit),
        )
        .route("/auth/logout", post(auth::logout))
        .route("/users", get(auth::users_list))
        .route(
            "/users/new",
            get(auth::new_user_page).post(auth::new_user_submit),
        )
        .route("/users/{id}/delete", post(auth::delete_user))
        .route("/users/{id}/promote", post(auth::promote_user))
        .with_state(auth_state)
        // Workout routes
        .route("/workouts", get(workouts::list))
        .route("/workouts/new", get(workouts::new_page))
        .route("/workouts", post(workouts::create))
        .route("/workouts/{id}", get(workouts::show))
        .route("/workouts/{id}/edit", get(workouts::edit_page))
        .route("/workouts/{id}", post(workouts::update))
        .route("/workouts/{id}/delete", post(workouts::delete))
        .route("/workouts/{id}/logs", post(workouts::add_log))
        .route(
            "/workouts/{id}/logs/{log_id}/delete",
            post(workouts::delete_log),
        )
        .route(
            "/workouts/{id}/logs/{log_id}/edit",
            get(workouts::edit_log_page),
        )
        .route("/workouts/{id}/logs/{log_id}", post(workouts::update_log))
        .route("/workouts/{id}/share", post(workouts::share_workout))
        .route("/workouts/{id}/revoke-share", post(workouts::revoke_share))
        // Public shared workout route (no auth required)
        .route("/shared/{token}", get(workouts::view_shared))
        .with_state(workouts_state)
        // Exercise routes
        .route("/exercises", get(exercises::list))
        .route("/exercises/new", get(exercises::new_page))
        .route("/exercises", post(exercises::create))
        .route("/exercises/{id}/edit", get(exercises::edit_page))
        .route("/exercises/{id}", post(exercises::update))
        .route("/exercises/{id}/delete", post(exercises::delete))
        .with_state(exercises_state)
        // Stats routes
        .route("/stats", get(stats::index))
        .route("/stats/exercise/{id}", get(stats::exercise_stats))
        .route("/stats/prs", get(stats::prs_list))
        .with_state(stats_state)
        // Settings routes
        .route("/settings", get(settings::index))
        .route("/settings/password", post(settings::change_password))
        .route("/settings/logout-others", post(settings::logout_others))
        .with_state(settings_state)
        // Sliding session: validate cookie, slide expiry, re-issue Set-Cookie on touch
        .layer(from_fn_with_state(
            session_repo.clone(),
            sliding_session_middleware,
        ))
        // Repos via Extension layer so extractors can pull them
        .layer(Extension(session_repo))
        .layer(Extension(user_repo))
}
