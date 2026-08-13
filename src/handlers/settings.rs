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
    let validation_error = if form.new_password != form.confirm_password {
        Some("New passwords do not match".to_string())
    } else if form.new_password == form.current_password {
        // Checked before the policy gate so the message is the specific one.
        // This is a correctness guard rather than a security control: the
        // request would otherwise "succeed" while changing nothing, tell the
        // user their password was changed, and destroy their other sessions —
        // all for a no-op. Someone rotating a possibly-compromised password
        // would walk away believing they had.
        Some("New password must be different from the current password".to_string())
    } else {
        // Same policy as signup and admin-created users; see
        // `password_policy_error`. The length ceiling also caps what reaches
        // Argon2 on this route, which runs it twice per request.
        //
        // `spawn_blocking` for the same reason as `validate_credentials`: the
        // strength check is CPU work on an attacker-chosen input.
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

    // Refund only now: the reservation stays charged for every path above
    // that did not prove knowledge of the current password.
    state
        .sensitive_action_rate_limiter
        .release(auth_user.id.clone());

    // OWASP Session Management Cheat Sheet, "Renew the Session ID After Any
    // Privilege Level Change" and the Authentication Cheat Sheet's
    // *Re-authentication After Risk Events* ("invalidate sessions after
    // re-authentication and rotate tokens"): a credential change is the risk
    // event. Destroying the *other* sessions was already happening; what was
    // missing is that the token in the user's own browser survived unchanged,
    // so a token captured before the change kept working after it — and
    // changing a password one believes to be compromised is exactly when that
    // matters.
    //
    // The new session is created *before* anything is destroyed: if `create`
    // fails, `?` returns having changed nothing but the password, leaving the
    // user logged in on their existing token rather than logged out with no
    // way back in. Passing the *new* token as the exception to
    // `delete_all_for_user_except` then retires the old current session in the
    // same statement as every other device, so there is no window in which
    // both tokens are live.
    let new_token = state.session_repo.create(&auth_user.id).await?;
    let new_fp = token_fingerprint(&new_token, state.log_salt.as_ref());
    let deleted_sessions = state
        .session_repo
        .delete_all_for_user_except(&auth_user.id, &new_token)
        .await?;

    // `count` now includes the rotated-away session, not just the other
    // devices — accurate, and the `session.created` event below names the
    // replacement, so the pair still reconciles.
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

    // The settings page marks the current row "This device" by comparing each
    // session's token against `user.session_token`, so the rendered identity
    // has to carry the new token — otherwise the page the user lands on shows
    // every session as somebody else's.
    let mut auth_user = auth_user;
    auth_user.session_token = new_token.clone();

    let mut response = render_page(
        &state,
        auth_user,
        None,
        Some("Password changed successfully. All other sessions have been logged out.".to_string()),
    )
    .await?;

    // Hand the browser the replacement cookie, and stop
    // `sliding_session_middleware` from appending a refresh for the token
    // that this request arrived with — that token no longer exists, and its
    // `Set-Cookie` would land after ours and log the user straight out.
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

/// Interstitial for `logout_others`. Counts the sessions that will actually
/// be dropped — "log out everywhere else" reads very differently when it is
/// about to end five sessions than when there are none.
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
