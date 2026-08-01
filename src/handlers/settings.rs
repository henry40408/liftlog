use askama::Template;
use axum::{
    Form,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;

use crate::audit::{self, AuditContext};
use crate::error::Result;
use crate::middleware::AuthUser;
use crate::models::password_length_error;
use crate::repositories::SessionListRow;
use crate::session::token_fingerprint;
use crate::state::AppState;
use crate::version::GIT_VERSION;

#[derive(Deserialize)]
pub struct ChangePasswordForm {
    pub current_password: String,
    pub new_password: String,
    pub confirm_password: String,
}

#[derive(Template)]
#[template(path = "settings/index.html")]
struct SettingsTemplate {
    user: AuthUser,
    git_version: &'static str,
    error: Option<String>,
    success: Option<String>,
    sessions: Vec<SessionListRow>,
}

async fn render_page(
    state: &AppState,
    auth_user: AuthUser,
    error: Option<String>,
    success: Option<String>,
) -> Result<Response> {
    render_page_with_status(state, auth_user, error, success, StatusCode::OK).await
}

/// `render_page`, but for the one caller that must not answer `200`: a
/// throttled password change is a refusal, and returning `200` would leave
/// automated clients (and any log-based alerting keyed on status) unable to
/// see that the request was rejected rather than processed.
async fn render_page_with_status(
    state: &AppState,
    auth_user: AuthUser,
    error: Option<String>,
    success: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let sessions = state.session_repo.list_for_user(&auth_user.id).await?;
    let template = SettingsTemplate {
        user: auth_user,
        git_version: GIT_VERSION,
        error,
        success,
        sessions,
    };
    Ok((status, Html(template.render()?)).into_response())
}

pub async fn index(State(state): State<AppState>, auth_user: AuthUser) -> Result<Response> {
    render_page(&state, auth_user, None, None).await
}

pub async fn change_password(
    State(state): State<AppState>,
    auth_user: AuthUser,
    audit_ctx: AuditContext,
    Form(form): Form<ChangePasswordForm>,
) -> Result<Response> {
    let validation_error = if form.new_password == form.confirm_password {
        // Same bounds as signup and admin-created users; see
        // `password_length_error`. The upper bound also caps what reaches
        // Argon2 on this route, which runs it twice per request.
        password_length_error(&form.new_password, "New password")
    } else {
        Some("New passwords do not match".to_string())
    };

    if let Some(message) = validation_error {
        return render_page(&state, auth_user, Some(message), None).await;
    }

    let actor_fp = token_fingerprint(&auth_user.session_token, state.log_salt.as_ref());

    // This is liftlog's *second* password-verification entry point, and until
    // now the only unthrottled one — an attacker holding a stolen session
    // cookie could guess `current_password` without limit, and each attempt
    // cost two Argon2 operations (19 MiB each) of server CPU and memory.
    //
    // Keyed by user id rather than client IP: the request is authenticated,
    // so the account under attack is known exactly, and an IP key would let
    // the same stolen session buy a fresh budget from every source address.
    // The budget is charged *before* verification, which is what bounds the
    // Argon2 work; it is handed back below only once the current password
    // proved correct, so a legitimate user changing their password repeatedly
    // is never locked out while a guesser's failures all stay charged.
    if !state
        .password_change_rate_limiter
        .try_acquire(auth_user.id.clone())
    {
        audit::password_change_throttled(&audit_ctx, &actor_fp, &auth_user.id);
        return render_page_with_status(
            &state,
            auth_user,
            Some("Too many password change attempts. Please try again later.".to_string()),
            None,
            StatusCode::TOO_MANY_REQUESTS,
        )
        .await;
    }

    let verified = state
        .user_repo
        .verify_password(&auth_user.username, &form.current_password)
        .await?;

    if verified.is_none() {
        audit::password_change_failed(&audit_ctx, &actor_fp, &auth_user.id);
        return render_page(
            &state,
            auth_user,
            Some("Current password is incorrect".to_string()),
            None,
        )
        .await;
    }

    state
        .user_repo
        .change_password(&auth_user.id, &form.new_password)
        .await?;

    // Refund only now: the reservation stays charged for every path above
    // that did not prove knowledge of the current password.
    state
        .password_change_rate_limiter
        .release(auth_user.id.clone());

    let deleted_sessions = state
        .session_repo
        .delete_all_for_user_except(&auth_user.id, &auth_user.session_token)
        .await?;
    audit::sessions_destroyed_bulk(
        &audit_ctx,
        &actor_fp,
        &auth_user.id,
        deleted_sessions,
        "password_change",
    );

    render_page(
        &state,
        auth_user,
        None,
        Some("Password changed successfully. All other sessions have been logged out.".to_string()),
    )
    .await
}

pub async fn logout_others(
    State(state): State<AppState>,
    auth_user: AuthUser,
    audit_ctx: AuditContext,
) -> Result<Response> {
    let deleted_sessions = state
        .session_repo
        .delete_all_for_user_except(&auth_user.id, &auth_user.session_token)
        .await?;
    let actor_fp = token_fingerprint(&auth_user.session_token, state.log_salt.as_ref());
    audit::sessions_destroyed_bulk(
        &audit_ctx,
        &actor_fp,
        &auth_user.id,
        deleted_sessions,
        "logout_others",
    );

    render_page(
        &state,
        auth_user,
        None,
        Some("Logged out of all other devices.".to_string()),
    )
    .await
}
