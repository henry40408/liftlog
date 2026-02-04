use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Response},
    Form,
};
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::middleware::AuthUser;
use crate::repositories::{SessionRepository, UserRepository};
use crate::version::GIT_VERSION;

#[derive(Clone)]
pub struct SettingsState {
    pub user_repo: UserRepository,
    pub session_repo: SessionRepository,
}

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
}

pub async fn index(auth_user: AuthUser) -> Result<Response> {
    let template = SettingsTemplate {
        user: auth_user,
        git_version: GIT_VERSION,
        error: None,
        success: None,
    };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn change_password(
    State(state): State<SettingsState>,
    auth_user: AuthUser,
    Form(form): Form<ChangePasswordForm>,
) -> Result<Response> {
    // Validate: passwords match
    if form.new_password != form.confirm_password {
        let template = SettingsTemplate {
            user: auth_user,
            git_version: GIT_VERSION,
            error: Some("New passwords do not match".to_string()),
            success: None,
        };
        return Ok(Html(
            template
                .render()
                .map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .into_response());
    }

    // Validate: minimum length
    if form.new_password.len() < 6 {
        let template = SettingsTemplate {
            user: auth_user,
            git_version: GIT_VERSION,
            error: Some("New password must be at least 6 characters".to_string()),
            success: None,
        };
        return Ok(Html(
            template
                .render()
                .map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .into_response());
    }

    // Verify current password
    let verified = state
        .user_repo
        .verify_password(&auth_user.username, &form.current_password)
        .await?;

    if verified.is_none() {
        let template = SettingsTemplate {
            user: auth_user,
            git_version: GIT_VERSION,
            error: Some("Current password is incorrect".to_string()),
            success: None,
        };
        return Ok(Html(
            template
                .render()
                .map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .into_response());
    }

    // Change password
    state
        .user_repo
        .change_password(&auth_user.id, &form.new_password)
        .await?;

    // Invalidate all other sessions
    state
        .session_repo
        .delete_all_for_user_except(&auth_user.id, &auth_user.session_token)
        .await?;

    let template = SettingsTemplate {
        user: auth_user,
        git_version: GIT_VERSION,
        error: None,
        success: Some(
            "Password changed successfully. All other sessions have been logged out.".to_string(),
        ),
    };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}
