//! Server-side confirmation interstitial for destructive actions, so they
//! still confirm with JavaScript off. User promote/delete use
//! `auth::render_confirm_page` instead, which also re-checks the password.

use askama::Template;
use axum::response::{Html, IntoResponse, Response};

use crate::error::Result;
use crate::middleware::AuthUser;

#[derive(Template)]
#[template(path = "confirm.html")]
struct ConfirmTemplate {
    user: AuthUser,
    action_label: &'static str,
    consequence: String,
    form_action: String,
    cancel_url: String,
}

/// `form_action` is the URL this page was served from: GET confirms, POST acts.
/// `consequence` should say what will be lost.
pub fn page(
    user: AuthUser,
    action_label: &'static str,
    consequence: String,
    form_action: String,
    cancel_url: String,
) -> Result<Response> {
    let template = ConfirmTemplate {
        user,
        action_label,
        consequence,
        form_action,
        cancel_url,
    };
    Ok(Html(template.render()?).into_response())
}
