use askama::Template;
use axum::{
    Form,
    extract::{Path, Request, State},
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;

use crate::audit::{self, AuditContext};
use crate::error::{AppError, Result};
use crate::middleware::auth::ValidatedSession;
use crate::middleware::{AdminUser, AuthUser, SuppressSessionRefresh};
use crate::models::{CreateUser, LoginCredentials, UserListItem, UserRole, password_policy_error};
use crate::session::{create_session_cookie, remove_session_cookie, token_fingerprint};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "auth/login.html")]
struct LoginTemplate {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "auth/setup.html")]
struct SetupTemplate {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "auth/new_user.html")]
struct NewUserTemplate {
    user: AuthUser,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "auth/confirm_action.html")]
struct ConfirmActionTemplate {
    user: AuthUser,
    action_label: &'static str,
    consequence: String,
    form_action: String,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "auth/users.html")]
struct UsersListTemplate {
    user: AuthUser,
    users: Vec<UserListItem>,
}

/// Returns the validation error message, or `None` if the form is valid.
///
/// Shares `password_policy_error` with the settings handler so both enforce
/// one policy. `spawn_blocking` because the strength check on an
/// attacker-chosen password can take milliseconds.
async fn validate_credentials(form: &CreateUser) -> Result<Option<String>> {
    if form.username.trim().is_empty() {
        return Ok(Some("Username is required".to_string()));
    }
    let password = form.password.clone();
    let username = form.username.clone();
    Ok(tokio::task::spawn_blocking(move || {
        password_policy_error(&password, "Password", &[username.as_str()])
    })
    .await?)
}

pub async fn login_page(State(state): State<AppState>, request: Request) -> Result<Response> {
    if request.extensions().get::<ValidatedSession>().is_some() {
        return Ok(Redirect::to("/").into_response());
    }

    let user_count = state.user_repo.count().await?;
    if user_count == 0 {
        return Ok(Redirect::to("/auth/setup").into_response());
    }

    let template = LoginTemplate { error: None };
    Ok(Html(template.render()?).into_response())
}

pub async fn login_submit(
    State(state): State<AppState>,
    connect: Option<axum::Extension<axum::extract::ConnectInfo<std::net::SocketAddr>>>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
    audit_ctx: AuditContext,
    Form(credentials): Form<LoginCredentials>,
) -> Result<Response> {
    let peer_addr = connect.map(|axum::Extension(axum::extract::ConnectInfo(addr))| addr.ip());
    let ip = crate::net::client_ip(
        peer_addr,
        &headers,
        state.trusted_proxy_header,
        &state.trusted_proxies,
    );

    if !state.login_rate_limiter.try_acquire(ip) {
        audit::login_throttled(&audit_ctx, &credentials.username);
        let template = LoginTemplate {
            error: Some("Too many login attempts. Please try again later.".to_string()),
        };
        return Ok((
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            Html(template.render()?),
        )
            .into_response());
    }

    // Delay applies to any submitted username, real or not, so it can't
    // reveal account existence; holding the connection is what rate-limits.
    let backoff = state.login_backoff.delay_for(&credentials.username);
    if !backoff.is_zero() {
        tokio::time::sleep(backoff).await;
    }

    let user = state
        .user_repo
        .verify_password(&credentials.username, &credentials.password)
        .await?;

    if let Some(user) = user {
        state.login_backoff.reset(&credentials.username);
        // Create the session before releasing the reservation, so a failed
        // create stays charged.
        let token = state.session_repo.create(&user.id).await?;
        state.login_rate_limiter.release(ip);
        audit::session_created(
            &audit_ctx,
            &token_fingerprint(&token, state.log_salt.as_ref()),
            &user.id,
            &user.username,
            "login",
        );
        let jar = jar.add(create_session_cookie(&token, state.cookie_secure));
        let mut response = (jar, Redirect::to("/")).into_response();
        // The middleware only sets Cache-Control for requests that already
        // had a session; this one sets the session cookie, so do it here.
        response.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        );
        response.headers_mut().insert(
            axum::http::header::PRAGMA,
            axum::http::HeaderValue::from_static("no-cache"),
        );
        Ok(response)
    } else {
        // One event for unknown user and wrong password alike, so the log
        // is not an enumeration oracle.
        state
            .login_backoff
            .record_failure(credentials.username.clone());
        // `backoff_ms` is the delay this attempt served.
        audit::login_failed(
            &audit_ctx,
            &credentials.username,
            u64::try_from(backoff.as_millis()).unwrap_or(u64::MAX),
        );
        let template = LoginTemplate {
            error: Some("Invalid username or password".to_string()),
        };
        Ok(Html(template.render()?).into_response())
    }
}

pub async fn setup_page(State(state): State<AppState>) -> Result<Response> {
    let user_count = state.user_repo.count().await?;
    if user_count > 0 {
        return Ok(Redirect::to("/auth/login").into_response());
    }

    let template = SetupTemplate { error: None };
    Ok(Html(template.render()?).into_response())
}

pub async fn setup_submit(
    State(state): State<AppState>,
    jar: CookieJar,
    audit_ctx: AuditContext,
    Form(form): Form<CreateUser>,
) -> Result<Response> {
    let user_count = state.user_repo.count().await?;
    if user_count > 0 {
        return Ok(Redirect::to("/auth/login").into_response());
    }

    if let Some(message) = validate_credentials(&form).await? {
        let template = SetupTemplate {
            error: Some(message),
        };
        return Ok(Html(template.render()?).into_response());
    }

    let user = state
        .user_repo
        .create(&form.username, &form.password, UserRole::Admin)
        .await?;

    let token = state.session_repo.create(&user.id).await?;
    audit::session_created(
        &audit_ctx,
        &token_fingerprint(&token, state.log_salt.as_ref()),
        &user.id,
        &user.username,
        "setup",
    );
    let jar = jar.add(create_session_cookie(&token, state.cookie_secure));

    let mut response = (jar, Redirect::to("/")).into_response();
    // See login_submit: the middleware won't set Cache-Control here.
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    response.headers_mut().insert(
        axum::http::header::PRAGMA,
        axum::http::HeaderValue::from_static("no-cache"),
    );
    Ok(response)
}

pub async fn logout(
    State(state): State<AppState>,
    auth_user: AuthUser,
    audit_ctx: AuditContext,
    jar: CookieJar,
) -> Response {
    let fp = token_fingerprint(&auth_user.session_token, state.log_salt.as_ref());
    match state.session_repo.delete(&auth_user.session_token).await {
        Ok(()) => audit::session_destroyed(&audit_ctx, &fp, &auth_user.id, "logout"),
        Err(e) => tracing::warn!(error = ?e, "logout: session delete failed"),
    }
    let jar = jar.add(remove_session_cookie(state.cookie_secure));
    let mut response = (jar, Redirect::to("/auth/login")).into_response();
    response.extensions_mut().insert(SuppressSessionRefresh);
    // Values must be quoted or browsers ignore the header. "executionContexts"
    // is omitted: its reload clashes with the redirect. Browsers ignore it on
    // insecure origins, so no `cookie_secure` gate.
    response.headers_mut().insert(
        axum::http::HeaderName::from_static("clear-site-data"),
        axum::http::HeaderValue::from_static("\"cache\", \"cookies\", \"storage\""),
    );
    response
}

pub async fn new_user_page(admin_user: AdminUser) -> Result<Response> {
    let template = NewUserTemplate {
        user: admin_user.0,
        error: None,
    };
    Ok(Html(template.render()?).into_response())
}

pub async fn new_user_submit(
    State(state): State<AppState>,
    admin_user: AdminUser,
    Form(form): Form<CreateUser>,
) -> Result<Response> {
    if let Some(message) = validate_credentials(&form).await? {
        let template = NewUserTemplate {
            user: admin_user.0,
            error: Some(message),
        };
        return Ok(Html(template.render()?).into_response());
    }

    if state
        .user_repo
        .find_by_username(&form.username)
        .await?
        .is_some()
    {
        let template = NewUserTemplate {
            user: admin_user.0,
            error: Some("Username already exists".to_string()),
        };
        return Ok(Html(template.render()?).into_response());
    }

    state
        .user_repo
        .create(&form.username, &form.password, UserRole::User)
        .await?;

    Ok(Redirect::to("/users").into_response())
}

pub async fn users_list(State(state): State<AppState>, auth_user: AuthUser) -> Result<Response> {
    let users = state.user_repo.find_all().await?;
    let template = UsersListTemplate {
        user: auth_user,
        users,
    };
    Ok(Html(template.render()?).into_response())
}

/// The admin's password, re-entered to confirm a user-management action.
#[derive(Debug, serde::Deserialize)]
pub struct ConfirmActionForm {
    pub current_password: String,
}

/// Keeps each action's wording, form target and audit name in one place.
#[derive(Clone, Copy)]
enum SensitiveAction {
    PromoteUser,
    DeleteUser,
}

impl SensitiveAction {
    fn label(self) -> &'static str {
        match self {
            Self::PromoteUser => "Promote to admin",
            Self::DeleteUser => "Delete user",
        }
    }

    fn consequence(self, username: &str) -> String {
        match self {
            Self::PromoteUser => format!(
                "{username} will gain full administrative access, including the ability to create and delete other users. They will be signed out of every device and must log in again."
            ),
            Self::DeleteUser => format!(
                "{username} and all of their workouts, exercises and sessions will be permanently deleted. This cannot be undone."
            ),
        }
    }

    fn form_action(self, user_id: &str) -> String {
        match self {
            Self::PromoteUser => format!("/users/{user_id}/promote"),
            Self::DeleteUser => format!("/users/{user_id}/delete"),
        }
    }

    /// Value of the `action` field on the `auth.reauth.*` audit events.
    fn audit_name(self) -> &'static str {
        match self {
            Self::PromoteUser => "promote_user",
            Self::DeleteUser => "delete_user",
        }
    }
}

/// Renders the re-auth confirmation page; a nonexistent target is a 404.
async fn render_confirm_page(
    state: &AppState,
    admin_user: AuthUser,
    action: SensitiveAction,
    target_id: &str,
    error: Option<String>,
) -> Result<Response> {
    let target = state
        .user_repo
        .find_by_id(target_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let template = ConfirmActionTemplate {
        user: admin_user,
        action_label: action.label(),
        consequence: action.consequence(&target.username),
        form_action: action.form_action(target_id),
        error,
    };
    Ok(Html(template.render()?).into_response())
}

/// Re-checks the admin's password before a sensitive action (OWASP
/// re-authentication), turning "has the cookie" into "knows the password".
/// Returns the confirmation page with an error on failure, `None` to proceed.
/// Shares the per-user throttle with password change.
async fn require_reauth(
    state: &AppState,
    admin_user: &AuthUser,
    audit_ctx: &AuditContext,
    action: SensitiveAction,
    target_id: &str,
    password: &str,
) -> Result<Option<Response>> {
    let actor_fp = token_fingerprint(&admin_user.session_token, state.log_salt.as_ref());

    if !state
        .sensitive_action_rate_limiter
        .try_acquire(admin_user.id.clone())
    {
        audit::reauth_throttled(audit_ctx, &actor_fp, &admin_user.id, action.audit_name());
        let page = render_confirm_page(
            state,
            admin_user.clone(),
            action,
            target_id,
            Some("Too many attempts. Please try again later.".to_string()),
        )
        .await?;
        return Ok(Some(
            (axum::http::StatusCode::TOO_MANY_REQUESTS, page).into_response(),
        ));
    }

    let verified = state
        .user_repo
        .verify_password(&admin_user.username, password)
        .await?;

    if verified.is_none() {
        audit::reauth_failed(audit_ctx, &actor_fp, &admin_user.id, action.audit_name());
        return Ok(Some(
            render_confirm_page(
                state,
                admin_user.clone(),
                action,
                target_id,
                Some("Password is incorrect".to_string()),
            )
            .await?,
        ));
    }

    // Only a proven password hands the attempt back, so failures stay charged.
    state
        .sensitive_action_rate_limiter
        .release(admin_user.id.clone());
    Ok(None)
}

pub async fn confirm_promote_page(
    State(state): State<AppState>,
    admin_user: AdminUser,
    Path(user_id): Path<String>,
) -> Result<Response> {
    render_confirm_page(
        &state,
        admin_user.0,
        SensitiveAction::PromoteUser,
        &user_id,
        None,
    )
    .await
}

pub async fn confirm_delete_page(
    State(state): State<AppState>,
    admin_user: AdminUser,
    Path(user_id): Path<String>,
) -> Result<Response> {
    if admin_user.id == user_id {
        return Err(AppError::BadRequest(
            "Cannot delete your own account".to_string(),
        ));
    }
    render_confirm_page(
        &state,
        admin_user.0,
        SensitiveAction::DeleteUser,
        &user_id,
        None,
    )
    .await
}

pub async fn delete_user(
    State(state): State<AppState>,
    admin_user: AdminUser,
    audit_ctx: AuditContext,
    Path(user_id): Path<String>,
    Form(form): Form<ConfirmActionForm>,
) -> Result<Response> {
    if admin_user.id == user_id {
        return Err(AppError::BadRequest(
            "Cannot delete your own account".to_string(),
        ));
    }

    if let Some(rejection) = require_reauth(
        &state,
        &admin_user.0,
        &audit_ctx,
        SensitiveAction::DeleteUser,
        &user_id,
        &form.current_password,
    )
    .await?
    {
        return Ok(rejection);
    }

    // Count before the delete: ON DELETE CASCADE removes the sessions with the
    // user row, leaving nothing to count afterwards.
    let sessions_destroyed = state.session_repo.count_for_user(&user_id).await?;

    // User row first, so a failure leaves sessions intact.
    let existed = state.user_repo.delete(&user_id).await?;
    if existed {
        // Normally a no-op after the cascade; covers a connection with
        // `foreign_keys` off.
        state.session_repo.delete_all_for_user(&user_id).await?;
        let actor_fp = token_fingerprint(&admin_user.session_token, state.log_salt.as_ref());
        audit::sessions_destroyed_bulk(
            &audit_ctx,
            &actor_fp,
            &user_id,
            sessions_destroyed,
            "admin_user_delete",
        );
    }

    Ok(Redirect::to("/users").into_response())
}

pub async fn promote_user(
    State(state): State<AppState>,
    admin_user: AdminUser,
    audit_ctx: AuditContext,
    Path(user_id): Path<String>,
    Form(form): Form<ConfirmActionForm>,
) -> Result<Response> {
    if let Some(rejection) = require_reauth(
        &state,
        &admin_user.0,
        &audit_ctx,
        SensitiveAction::PromoteUser,
        &user_id,
        &form.current_password,
    )
    .await?
    {
        return Ok(rejection);
    }

    let promoted = state
        .user_repo
        .update_role(&user_id, UserRole::Admin)
        .await?;

    // Renew sessions on privilege change (OWASP): a token stolen before the
    // promotion must not inherit admin. Gated on `promoted` so a missing id
    // logs nothing.
    if promoted {
        let destroyed = state.session_repo.delete_all_for_user(&user_id).await?;
        let actor_fp = token_fingerprint(&admin_user.session_token, state.log_salt.as_ref());
        audit::sessions_destroyed_bulk(&audit_ctx, &actor_fp, &user_id, destroyed, "role_change");
    }

    Ok(Redirect::to("/users").into_response())
}
