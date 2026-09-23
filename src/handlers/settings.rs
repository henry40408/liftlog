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
use crate::handlers::confirm;
use crate::middleware::{AuthUser, SuppressSessionRefresh};
use crate::models::password_policy_error;
use crate::repositories::SessionListRow;
use crate::session::{create_session_cookie, token_fingerprint};
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

/// `render_page` with a non-200 status, so a throttled change reads as refused.
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
    let validation_error = if form.new_password != form.confirm_password {
        Some("New passwords do not match".to_string())
    } else if form.new_password == form.current_password {
        // Otherwise a no-op "change" would report success and drop sessions.
        Some("New password must be different from the current password".to_string())
    } else {
        // Shared policy (`password_policy_error`); the length cap also bounds
        // Argon2 work. `spawn_blocking`: CPU work on attacker input.
        let new_password = form.new_password.clone();
        let username = auth_user.username.clone();
        tokio::task::spawn_blocking(move || {
            password_policy_error(&new_password, "New password", &[username.as_str()])
        })
        .await?
    };

    if let Some(message) = validation_error {
        return render_page(&state, auth_user, Some(message), None).await;
    }

    let actor_fp = token_fingerprint(&auth_user.session_token, state.log_salt.as_ref());

    // Throttle current-password guessing with a stolen cookie. Keyed by user
    // id so new IPs don't buy fresh budget; charged before verifying and
    // refunded only on success.
    if !state
        .sensitive_action_rate_limiter
        .try_acquire(auth_user.id.clone())
    {
        audit::reauth_throttled(&audit_ctx, &actor_fp, &auth_user.id, "password_change");
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
        audit::reauth_failed(&audit_ctx, &actor_fp, &auth_user.id, "password_change");
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

    state
        .sensitive_action_rate_limiter
        .release(auth_user.id.clone());

    // Rotate the current token too (OWASP: renew after risk events), so a
    // token captured before the change stops working. Create first: if it
    // fails the user keeps their old session. Excluding only the new token
    // retires the old one with every other device in one statement.
    let new_token = state.session_repo.create(&auth_user.id).await?;
    let new_fp = token_fingerprint(&new_token, state.log_salt.as_ref());
    let deleted_sessions = state
        .session_repo
        .delete_all_for_user_except(&auth_user.id, &new_token)
        .await?;

    // `count` includes the rotated-away session.
    audit::sessions_destroyed_bulk(
        &audit_ctx,
        &actor_fp,
        &auth_user.id,
        deleted_sessions,
        "password_change",
    );
    audit::session_created(
        &audit_ctx,
        &new_fp,
        &auth_user.id,
        &auth_user.username,
        "password_change_rotation",
    );

    // The page marks "This device" by token, so render with the new one.
    let mut auth_user = auth_user;
    auth_user.session_token = new_token.clone();

    let mut response = render_page(
        &state,
        auth_user,
        None,
        Some("Password changed successfully. All other sessions have been logged out.".to_string()),
    )
    .await?;

    // Suppress the middleware's refresh of the now-deleted old token, whose
    // Set-Cookie would land after ours and log the user out.
    let cookie = create_session_cookie(&new_token, state.cookie_secure);
    response.headers_mut().append(
        axum::http::header::SET_COOKIE,
        cookie
            .to_string()
            .parse()
            .expect("session cookie serialises to a valid header value"),
    );
    response.extensions_mut().insert(SuppressSessionRefresh);
    Ok(response)
}

/// Interstitial for `logout_others`; shows how many sessions will end.
pub async fn confirm_logout_others(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Response> {
    let others = state
        .session_repo
        .list_for_user(&auth_user.id)
        .await?
        .into_iter()
        .filter(|s| s.token != auth_user.session_token)
        .count();

    let consequence = match others {
        0 => "No other device is signed in, so nothing will be logged out.".to_string(),
        1 => {
            "1 other signed-in device will be logged out. This device stays signed in.".to_string()
        }
        n => {
            format!("{n} other signed-in devices will be logged out. This device stays signed in.")
        }
    };

    confirm::page(
        auth_user,
        "Log out other devices",
        consequence,
        "/settings/logout-others".to_string(),
        "/settings".to_string(),
    )
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
