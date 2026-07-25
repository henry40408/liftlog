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
use crate::models::{CreateUser, LoginCredentials, User, UserRole};
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
#[template(path = "auth/users.html")]
struct UsersListTemplate {
    user: AuthUser,
    users: Vec<User>,
}

/// Returns the validation error message, or `None` if the form is valid.
fn validate_credentials(form: &CreateUser) -> Option<&'static str> {
    if form.username.trim().is_empty() {
        Some("Username is required")
    } else if form.password.len() < 6 {
        Some("Password must be at least 6 characters")
    } else {
        None
    }
}

pub async fn login_page(State(state): State<AppState>, request: Request) -> Result<Response> {
    // sliding_session_middleware injects ValidatedSession into request
    // extensions when the cookie is valid; bounce already-logged-in users.
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
        tracing::warn!(%ip, "login rate limited");
        let template = LoginTemplate {
            error: Some("Too many login attempts. Please try again later.".to_string()),
        };
        return Ok((
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            Html(template.render()?),
        )
            .into_response());
    }

    let user = state
        .user_repo
        .verify_password(&credentials.username, &credentials.password)
        .await?;

    if let Some(user) = user {
        // Create the session before releasing the rate-limit reservation:
        // if `session_repo.create` fails, `?` below returns early and the
        // attempt stays charged instead of being refunded for a login that
        // never actually completed.
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
        Ok((jar, Redirect::to("/")).into_response())
    } else {
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

    if let Some(message) = validate_credentials(&form) {
        let template = SetupTemplate {
            error: Some(message.to_string()),
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

    Ok((jar, Redirect::to("/")).into_response())
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
    // Tell sliding_session_middleware not to overwrite the removal cookie
    // with a refreshed one.
    response.extensions_mut().insert(SuppressSessionRefresh);
    // OWASP Session Management Cheat Sheet (Manual Session Expiration): ask
    // the browser to drop the site's cookies, cache and local storage on
    // logout, not just the one session cookie above — covers anything else
    // an XSS or a shared machine could have stashed. The quotes around each
    // directive are required syntax (a comma-separated list of quoted
    // strings) — sending `cache, cookies, storage` unquoted makes browsers
    // ignore the header entirely, silently. Deliberately omitting
    // "executionContexts": it asks the browser to reload the associated
    // browsing contexts, which interacts inconsistently across browsers with
    // the redirect this handler already issues. Sent unconditionally
    // (not gated on `state.cookie_secure`) because browsers already ignore
    // this header on non-secure origins, so a conditional here would only
    // add a branch without changing behaviour.
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
    if let Some(message) = validate_credentials(&form) {
        let template = NewUserTemplate {
            user: admin_user.0,
            error: Some(message.to_string()),
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

pub async fn delete_user(
    State(state): State<AppState>,
    admin_user: AdminUser,
    audit_ctx: AuditContext,
    Path(user_id): Path<String>,
) -> Result<Response> {
    if admin_user.id == user_id {
        return Err(AppError::BadRequest(
            "Cannot delete your own account".to_string(),
        ));
    }

    // Read the session count *before* the delete. It cannot be derived from
    // the explicit cleanup below: `sessions.user_id` is ON DELETE CASCADE, so
    // wherever `PRAGMA foreign_keys` is enforced the cascade removes those
    // rows as part of the `users` delete and the cleanup finds nothing left
    // to count — which would log `count: 0` for an action that really did
    // destroy sessions. Enforcement is currently off on every pooled
    // connection, but it is a per-connection setting and is intended to be
    // enabled uniformly in `src/db.rs`, so this must be correct either way.
    let sessions_destroyed = state.session_repo.count_for_user(&user_id).await?;

    // The user row goes first. If it fails, `?` returns having changed
    // nothing — whereas cleaning sessions up first would leave a surviving
    // account with every session destroyed and an audit line claiming the
    // account was deleted.
    let existed = state.user_repo.delete(&user_id).await?;
    if existed {
        // Mop up whatever the cascade did not take. Required while FK
        // enforcement is off on most pooled connections; a harmless no-op
        // once it is on everywhere. Orphaned rows cannot authenticate —
        // `validate_and_touch` INNER JOINs `users` — but they would
        // otherwise sit until the hourly sweep retires them.
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
    _admin_user: AdminUser,
    Path(user_id): Path<String>,
) -> Result<Response> {
    state
        .user_repo
        .update_role(&user_id, UserRole::Admin)
        .await?;

    Ok(Redirect::to("/users").into_response())
}
