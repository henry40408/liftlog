use askama::Template;
use axum::{
    extract::{Path, Request, State},
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use axum_extra::extract::CookieJar;

use crate::error::{AppError, Result};
use crate::middleware::auth::ValidatedSession;
use crate::middleware::{AdminUser, AuthUser, SuppressSessionRefresh};
use crate::models::{CreateUser, LoginCredentials, User, UserRole};
use crate::session::{create_session_cookie, remove_session_cookie};
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
    jar: CookieJar,
    Form(credentials): Form<LoginCredentials>,
) -> Result<Response> {
    let user = state
        .user_repo
        .verify_password(&credentials.username, &credentials.password)
        .await?;

    match user {
        Some(user) => {
            let token = state.session_repo.create(&user.id).await?;
            let jar = jar.add(create_session_cookie(&token));
            Ok((jar, Redirect::to("/")).into_response())
        }
        None => {
            let template = LoginTemplate {
                error: Some("Invalid username or password".to_string()),
            };
            Ok(Html(template.render()?).into_response())
        }
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
    let jar = jar.add(create_session_cookie(&token));

    Ok((jar, Redirect::to("/")).into_response())
}

pub async fn logout(
    State(state): State<AppState>,
    auth_user: AuthUser,
    jar: CookieJar,
) -> Response {
    let _ = state.session_repo.delete(&auth_user.session_token).await;
    let jar = jar.add(remove_session_cookie());
    let mut response = (jar, Redirect::to("/auth/login")).into_response();
    // Tell sliding_session_middleware not to overwrite the removal cookie
    // with a refreshed one.
    response.extensions_mut().insert(SuppressSessionRefresh);
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
    Path(user_id): Path<String>,
) -> Result<Response> {
    if admin_user.id == user_id {
        return Err(AppError::BadRequest(
            "Cannot delete your own account".to_string(),
        ));
    }

    state.user_repo.delete(&user_id).await?;

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
