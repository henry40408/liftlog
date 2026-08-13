//! The interstitial that stands in for `window.confirm()`.
//!
//! Destructive actions used to be POST forms carrying
//! `onsubmit="return confirm(...)"`. That handler never runs with JavaScript
//! off, so the form submitted straight through and the action happened on the
//! first click with nothing asked — worst of all for deleting a workout,
//! which cascades to every set in it. Routing the trigger through a GET page
//! keeps the confirmation in the server's hands, where it does not depend on
//! the browser running scripts.
//!
//! `auth::confirm_delete_page` stays separate: promoting or deleting a *user*
//! also re-checks the admin's own password, which these actions do not.

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

/// Renders the confirmation page.
///
/// `form_action` is the same URL the page was reached at, so every action
/// keeps one route serving `get` (this page) and `post` (the deed itself) —
/// the shape `/users/{id}/delete` already uses.
///
/// `consequence` is a full sentence rather than a label: an interstitial is
/// only worth the extra page load if it says what is about to be lost.
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
