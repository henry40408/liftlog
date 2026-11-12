//! Server-side confirmation interstitial for destructive actions, so they
//! still confirm with JavaScript off. User promote/delete use
//! `auth::render_confirm_page` instead, which also re-checks the password.

use axum::response::{IntoResponse, Response};

use crate::middleware::AuthUser;
use crate::views::confirm::{ConfirmView, confirm};

/// `form_action` is the URL this page was served from: GET confirms, POST acts.
/// `consequence` should say what will be lost.
pub fn page(
    user: AuthUser,
    action_label: &'static str,
    consequence: String,
    form_action: String,
    cancel_url: String,
) -> Response {
    let view = ConfirmView {
        user: &user,
        action_label,
        consequence: &consequence,
        form_action: &form_action,
        cancel_url: &cancel_url,
    };
    confirm(&view).into_response()
}
