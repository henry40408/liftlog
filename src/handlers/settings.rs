use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Response},
    Form,
};
use serde::Deserialize;

use crate::error::Result;
use crate::middleware::AuthUser;
use crate::repositories::SessionListRow;
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
    let sessions = state.session_repo.list_for_user(&auth_user.id).await?;
    let template = SettingsTemplate {
        user: auth_user,
        git_version: GIT_VERSION,
        error,
        success,
        sessions,
    };
    Ok(Html(template.render()?).into_response())
}

pub async fn index(State(state): State<AppState>, auth_user: AuthUser) -> Result<Response> {
    render_page(&state, auth_user, None, None).await
}

pub async fn change_password(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Form(form): Form<ChangePasswordForm>,
) -> Result<Response> {
    let validation_error = if form.new_password != form.confirm_password {
        Some("New passwords do not match")
    } else if form.new_password.len() < 6 {
        Some("New password must be at least 6 characters")
    } else {
        None
    };

    if let Some(message) = validation_error {
        return render_page(&state, auth_user, Some(message.to_string()), None).await;
    }

    let verified = state
        .user_repo
        .verify_password(&auth_user.username, &form.current_password)
        .await?;

    if verified.is_none() {
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
        .session_repo
        .delete_all_for_user_except(&auth_user.id, &auth_user.session_token)
        .await?;

    render_page(
        &state,
        auth_user,
        None,
        Some("Password changed successfully. All other sessions have been logged out.".to_string()),
    )
    .await
}

pub async fn logout_others(State(state): State<AppState>, auth_user: AuthUser) -> Result<Response> {
    state
        .session_repo
        .delete_all_for_user_except(&auth_user.id, &auth_user.session_token)
        .await?;

    render_page(
        &state,
        auth_user,
        None,
        Some("Logged out of all other devices.".to_string()),
    )
    .await
}
