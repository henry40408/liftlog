use askama::Template;
use axum::response::{Html, IntoResponse, Response};

use crate::error::{AppError, Result};
use crate::middleware::AuthUser;
use crate::version::GIT_VERSION;

#[derive(Template)]
#[template(path = "settings/index.html")]
struct SettingsTemplate {
    user: AuthUser,
    git_version: &'static str,
}

pub async fn index(auth_user: AuthUser) -> Result<Response> {
    let template = SettingsTemplate {
        user: auth_user,
        git_version: GIT_VERSION,
    };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}
