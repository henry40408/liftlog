use axum::{
    Router,
    middleware::{from_fn, from_fn_with_state},
    routing::{get, post},
};

use crate::handlers::{auth, dashboard, exercises, favicon, health, settings, stats, workouts};
use crate::middleware::{
    HstsHeader, SessionLayerState, baseline_headers_middleware, csrf_origin_guard, hsts_middleware,
    sliding_session_middleware,
};
use crate::state::AppState;

pub fn create_router(state: AppState) -> Router {
    let session_layer_state = SessionLayerState {
        session_repo: state.session_repo.clone(),
        cookie_secure: state.cookie_secure,
        log_salt: state.log_salt.clone(),
        trusted_proxy_header: state.trusted_proxy_header,
        trusted_proxies: state.trusted_proxies.clone(),
    };
    // Read off `state` before `.with_state(state)` moves it below.
    let hsts_max_age = state.hsts_max_age;
    let hsts_include_subdomains = state.hsts_include_subdomains;

    Router::new()
        .route("/health", get(health::health_check))
        // Favicon (no auth, no state)
        .route("/favicon.svg", get(favicon::favicon_svg))
        .route("/apple-touch-icon.png", get(favicon::apple_touch_icon))
        .route("/", get(dashboard::index))
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
        // GET renders a confirmation page that re-checks the admin's own
        // password; POST is the action itself and refuses without it.
        .route(
            "/users/{id}/delete",
            get(auth::confirm_delete_page).post(auth::delete_user),
        )
        .route(
            "/users/{id}/promote",
            get(auth::confirm_promote_page).post(auth::promote_user),
        )
        .route("/workouts", get(workouts::list))
        .route("/workouts/new", get(workouts::new_page))
        .route("/workouts", post(workouts::create))
        .route("/workouts/{id}", get(workouts::show))
        .route("/workouts/{id}/edit", get(workouts::edit_page))
        .route("/workouts/{id}", post(workouts::update))
        .route(
            "/workouts/{id}/delete",
            get(workouts::confirm_delete).post(workouts::delete),
        )
        .route("/workouts/{id}/logs", post(workouts::add_log))
        .route(
            "/workouts/{id}/logs/{log_id}/delete",
            get(workouts::confirm_delete_log).post(workouts::delete_log),
        )
        .route(
            "/workouts/{id}/logs/{log_id}/edit",
            get(workouts::edit_log_page),
        )
        .route("/workouts/{id}/logs/{log_id}", post(workouts::update_log))
        .route("/workouts/{id}/share", post(workouts::share_workout))
        .route(
            "/workouts/{id}/revoke-share",
            get(workouts::confirm_revoke_share).post(workouts::revoke_share),
        )
        // Public shared workout route (no auth required)
        .route("/shared/{token}", get(workouts::view_shared))
        .route("/exercises", get(exercises::list))
        .route("/exercises/new", get(exercises::new_page))
        .route("/exercises", post(exercises::create))
        .route("/exercises/{id}/edit", get(exercises::edit_page))
        .route("/exercises/{id}", post(exercises::update))
        .route(
            "/exercises/{id}/delete",
            get(exercises::confirm_delete).post(exercises::delete),
        )
        .route("/stats", get(stats::index))
        .route("/stats/exercise/{id}", get(stats::exercise_stats))
        .route("/stats/prs", get(stats::prs_list))
        .route("/settings", get(settings::index))
        .route("/settings/password", post(settings::change_password))
        .route(
            "/settings/logout-others",
            get(settings::confirm_logout_others).post(settings::logout_others),
        )
        .with_state(state)
        // Sliding session: validate cookie, slide expiry, re-issue Set-Cookie on touch
        .layer(from_fn_with_state(
            session_layer_state,
            sliding_session_middleware,
        ))
        // First-line CSRF: reject provably cross-site state-changing requests.
        // Registered before HSTS below → runs before session validation, and
        // after HSTS in request order (outer layers run first).
        .layer(from_fn(csrf_origin_guard))
        // Baseline security headers, outside the CSRF guard for the same
        // reason HSTS is: the 403 that guard returns must carry them too.
        .layer(from_fn(baseline_headers_middleware))
        // HSTS must be the outermost layer: it is registered last, after the
        // CSRF guard, so it also stamps responses that short-circuit inside
        // that guard (its 403) or inside session validation (the AuthRedirect
        // 302) rather than only ones that reach a handler. Any layer inside
        // this one that returns early would ship without the header.
        .layer(from_fn_with_state(
            HstsHeader::new(hsts_max_age, hsts_include_subdomains),
            hsts_middleware,
        ))
}
