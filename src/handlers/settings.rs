use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Response},
    Form,
};
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::middleware::AuthUser;
use crate::repositories::{SessionListRow, SessionRepository, UserRepository};
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
    sessions: Vec<SessionListRow>,
    current_token: String,
}

fn render_settings(template: SettingsTemplate) -> Result<Response> {
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn index(State(state): State<SettingsState>, auth_user: AuthUser) -> Result<Response> {
    let sessions = state.session_repo.list_for_user(&auth_user.id).await?;
    let current_token = auth_user.session_token.clone();
    render_settings(SettingsTemplate {
        user: auth_user,
        git_version: GIT_VERSION,
        error: None,
        success: None,
        sessions,
        current_token,
    })
}

pub async fn change_password(
    State(state): State<SettingsState>,
    auth_user: AuthUser,
    Form(form): Form<ChangePasswordForm>,
) -> Result<Response> {
    let sessions = state.session_repo.list_for_user(&auth_user.id).await?;
    let current_token = auth_user.session_token.clone();

    // Validate: passwords match
    if form.new_password != form.confirm_password {
        return render_settings(SettingsTemplate {
            user: auth_user,
            git_version: GIT_VERSION,
            error: Some("New passwords do not match".to_string()),
            success: None,
            sessions,
            current_token,
        });
    }

    // Validate: minimum length
    if form.new_password.len() < 6 {
        return render_settings(SettingsTemplate {
            user: auth_user,
            git_version: GIT_VERSION,
            error: Some("New password must be at least 6 characters".to_string()),
            success: None,
            sessions,
            current_token,
        });
    }

    // Verify current password
    let verified = state
        .user_repo
        .verify_password(&auth_user.username, &form.current_password)
        .await?;

    if verified.is_none() {
        return render_settings(SettingsTemplate {
            user: auth_user,
            git_version: GIT_VERSION,
            error: Some("Current password is incorrect".to_string()),
            success: None,
            sessions,
            current_token,
        });
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

    // Reload sessions so the rendered list reflects the just-revoked siblings.
    let fresh_sessions = state.session_repo.list_for_user(&auth_user.id).await?;
    render_settings(SettingsTemplate {
        user: auth_user,
        git_version: GIT_VERSION,
        error: None,
        success: Some(
            "Password changed successfully. All other sessions have been logged out.".to_string(),
        ),
        sessions: fresh_sessions,
        current_token,
    })
}

pub async fn logout_others(
    State(state): State<SettingsState>,
    auth_user: AuthUser,
) -> Result<Response> {
    state
        .session_repo
        .delete_all_for_user_except(&auth_user.id, &auth_user.session_token)
        .await?;

    let sessions = state.session_repo.list_for_user(&auth_user.id).await?;
    let current_token = auth_user.session_token.clone();
    render_settings(SettingsTemplate {
        user: auth_user,
        git_version: GIT_VERSION,
        error: None,
        success: Some("Logged out of all other devices.".to_string()),
        sessions,
        current_token,
    })
}
