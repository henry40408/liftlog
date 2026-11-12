use maud::{Markup, html};

use super::layout::{nav, page};
use crate::middleware::AuthUser;

pub struct ConfirmView<'a> {
    pub user: &'a AuthUser,
    pub action_label: &'a str,
    pub consequence: &'a str,
    pub form_action: &'a str,
    pub cancel_url: &'a str,
}

/// Server-side confirmation interstitial: GET shows it, only the POST acts.
pub fn confirm(view: &ConfirmView) -> Markup {
    let body = html! {
        (nav(view.user))
        main {
            div class="page-header" { h1 { (view.action_label) } }
            p class="mb-4" { (view.consequence) }
            div class="actions" {
                form method="post" action=(view.form_action) {
                    button type="submit" class="btn btn-danger" { (view.action_label) }
                }
                a href=(view.cancel_url) class="btn btn-ghost" { "Cancel" }
            }
        }
    };
    page(&format!("{} - LiftLog", view.action_label), body)
}
