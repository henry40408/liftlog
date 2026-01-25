use askama::Template;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect, Response},
    Extension, Form,
};
use axum_extra::extract::cookie::SignedCookieJar;

use crate::error::{AppError, Result};
use crate::middleware::{auth::OptionalAuthUser, AdminUser, AuthUser};
use crate::models::{CreateUser, LoginCredentials, User, UserRole};
use crate::repositories::UserRepository;
use crate::session::SessionKey;

#[derive(Clone)]
pub struct AuthState {
    pub user_repo: UserRepository,
}

// Templates
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
    is_admin: bool,
}

// Handlers
pub async fn login_page(
    State(state): State<AuthState>,
    OptionalAuthUser(auth_user): OptionalAuthUser,
) -> Result<Response> {
    // Redirect to dashboard if already logged in
    if auth_user.is_some() {
        return Ok(Redirect::to("/").into_response());
    }

    // Check if any users exist
    let user_count = state.user_repo.count().await?;
    if user_count == 0 {
        return Ok(Redirect::to("/auth/setup").into_response());
    }

    let template = LoginTemplate { error: None };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn login_submit(
    State(state): State<AuthState>,
    Extension(key): Extension<SessionKey>,
    headers: HeaderMap,
    Form(credentials): Form<LoginCredentials>,
) -> Result<Response> {
    let jar = SignedCookieJar::from_headers(&headers, key.0);

    let user = state
        .user_repo
        .verify_password(&credentials.username, &credentials.password)
        .await?;

    match user {
        Some(user) => {
            let jar = AuthUser::login(jar, &user);
            Ok((jar, Redirect::to("/")).into_response())
        }
        None => {
            let template = LoginTemplate {
                error: Some("Invalid username or password".to_string()),
            };
            Ok((
                jar,
                Html(
                    template
                        .render()
                        .map_err(|e| AppError::Internal(e.to_string()))?,
                ),
            )
                .into_response())
        }
    }
}

pub async fn setup_page(State(state): State<AuthState>) -> Result<Response> {
    // Only allow setup if no users exist
    let user_count = state.user_repo.count().await?;
    if user_count > 0 {
        return Ok(Redirect::to("/auth/login").into_response());
    }

    let template = SetupTemplate { error: None };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn setup_submit(
    State(state): State<AuthState>,
    Extension(key): Extension<SessionKey>,
    headers: HeaderMap,
    Form(form): Form<CreateUser>,
) -> Result<Response> {
    let jar = SignedCookieJar::from_headers(&headers, key.0);

    // Only allow setup if no users exist
    let user_count = state.user_repo.count().await?;
    if user_count > 0 {
        return Ok((jar, Redirect::to("/auth/login")).into_response());
    }

    // Validate input
    if form.username.trim().is_empty() {
        let template = SetupTemplate {
            error: Some("Username is required".to_string()),
        };
        return Ok((
            jar,
            Html(
                template
                    .render()
                    .map_err(|e| AppError::Internal(e.to_string()))?,
            ),
        )
            .into_response());
    }

    if form.password.len() < 6 {
        let template = SetupTemplate {
            error: Some("Password must be at least 6 characters".to_string()),
        };
        return Ok((
            jar,
            Html(
                template
                    .render()
                    .map_err(|e| AppError::Internal(e.to_string()))?,
            ),
        )
            .into_response());
    }

    // Create the first user as admin
    let user = state
        .user_repo
        .create(&form.username, &form.password, UserRole::Admin)
        .await?;

    // Auto login
    let jar = AuthUser::login(jar, &user);

    Ok((jar, Redirect::to("/")).into_response())
}

pub async fn logout(Extension(key): Extension<SessionKey>, headers: HeaderMap) -> Response {
    let jar = SignedCookieJar::from_headers(&headers, key.0);
    let jar = AuthUser::logout(jar);
    (jar, Redirect::to("/auth/login")).into_response()
}

pub async fn new_user_page(admin_user: AdminUser) -> Result<Response> {
    let template = NewUserTemplate {
        user: admin_user.0,
        error: None,
    };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn new_user_submit(
    State(state): State<AuthState>,
    admin_user: AdminUser,
    Form(form): Form<CreateUser>,
) -> Result<Response> {
    // Validate input
    if form.username.trim().is_empty() {
        let template = NewUserTemplate {
            user: admin_user.0,
            error: Some("Username is required".to_string()),
        };
        return Ok(Html(
            template
                .render()
                .map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .into_response());
    }

    if form.password.len() < 6 {
        let template = NewUserTemplate {
            user: admin_user.0,
            error: Some("Password must be at least 6 characters".to_string()),
        };
        return Ok(Html(
            template
                .render()
                .map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .into_response());
    }

    // Check if username already exists
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
        return Ok(Html(
            template
                .render()
                .map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .into_response());
    }

    // Create user with regular user role
    state
        .user_repo
        .create(&form.username, &form.password, UserRole::User)
        .await?;

    Ok(Redirect::to("/users").into_response())
}

pub async fn users_list(State(state): State<AuthState>, auth_user: AuthUser) -> Result<Response> {
    let users = state.user_repo.find_all().await?;
    let is_admin = auth_user.is_admin();
    let template = UsersListTemplate {
        user: auth_user,
        users,
        is_admin,
    };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn delete_user(
    State(state): State<AuthState>,
    admin_user: AdminUser,
    Path(user_id): Path<String>,
) -> Result<Response> {
    // Prevent self-delete
    if admin_user.id == user_id {
        return Err(AppError::BadRequest(
            "Cannot delete your own account".to_string(),
        ));
    }

    state.user_repo.delete(&user_id).await?;

    Ok(Redirect::to("/users").into_response())
}

pub async fn promote_user(
    State(state): State<AuthState>,
    _admin_user: AdminUser,
    Path(user_id): Path<String>,
) -> Result<Response> {
    state
        .user_repo
        .update_role(&user_id, UserRole::Admin)
        .await?;

    Ok(Redirect::to("/users").into_response())
}
