use maud::{Markup, PreEscaped, html};

use super::layout::{nav, page};
use crate::middleware::AuthUser;
use crate::repositories::SessionListRow;

pub struct SettingsView<'a> {
    pub user: &'a AuthUser,
    pub timezone: Option<&'a str>,
    pub timezone_names: &'a [&'static str],
    pub git_version: &'a str,
    pub error: Option<&'a str>,
    pub success: Option<&'a str>,
    pub sessions: &'a [SessionListRow],
}

/// Prefills the browser's zone only when none is saved yet.
const TIMEZONE_PREFILL_JS: &str = "
        /* Prefill with the browser's zone only when none is saved yet. */
        (function () {
            var input = document.getElementById('timezone-input');
            if (!input || input.value) return;
            try {
                input.value = Intl.DateTimeFormat().resolvedOptions().timeZone || '';
            } catch (e) { /* leave empty */ }
        })();
    ";

pub fn settings(view: &SettingsView) -> Markup {
    let body = html! {
        (nav(view.user))
        main {
            div class="page-header" { h1 { "Settings" } }
            @if let Some(msg) = view.success {
                div class="alert alert-success" style="margin-bottom: var(--sp-5);" { (msg) }
            }
            @if let Some(err) = view.error { div class="error" { (err) } }
            (timezone_section(view))
            (password_section())
            (sessions_section(view))
            h2 { "Application Info" }
            table class="data-table" {
                tbody { tr {
                    td style="color: var(--text-muted); width: 150px;" { "Version" }
                    td { code { (view.git_version) } }
                } }
            }
        }
    };
    page("Settings - LiftLog", body)
}

fn timezone_section(view: &SettingsView) -> Markup {
    html! {
        h2 id="timezone" { "Timezone" }
        p class="muted" { "Workout dates and the \"This Week\" / \"This Month\" stats follow this timezone." }
        form method="post" action="/settings/timezone" {
            div class="form-group" {
                label for="timezone-input" { "Timezone (IANA name)" }
                " "
                input type="text" id="timezone-input" name="timezone" list="timezone-names" required
                    autocomplete="off" value=(view.timezone.unwrap_or("")) placeholder="Asia/Taipei";
                datalist id="timezone-names" {
                    @for name in view.timezone_names { option value=(name) {} }
                }
            }
            button type="submit" { "Save Timezone" }
        }
        script { (PreEscaped(TIMEZONE_PREFILL_JS)) }
    }
}

fn password_section() -> Markup {
    html! {
        h2 { "Change Password" }
        form method="post" action="/settings/password" {
            div class="form-group" {
                label for="current_password" { "Current Password" }
                " "
                input type="password" id="current_password" name="current_password" required autocomplete="current-password";
            }
            div class="form-group" {
                label for="new_password" { "New Password" }
                " "
                input type="password" id="new_password" name="new_password" required minlength="12" maxlength="128" autocomplete="new-password";
            }
            div class="form-group" {
                label for="confirm_password" { "Confirm New Password" }
                " "
                input type="password" id="confirm_password" name="confirm_password" required minlength="12" maxlength="128" autocomplete="new-password";
            }
            button type="submit" { "Change Password" }
        }
    }
}

fn sessions_section(view: &SettingsView) -> Markup {
    html! {
        h2 { "Active Sessions" }
        table class="data-table" {
            thead { tr { th { "Device" } th { "Last active" } th { "Signed in" } } }
            tbody {
                @for s in view.sessions {
                    tr {
                        td data-label="Device" {
                            @if s.token == view.user.session_token { strong { "This device" } } @else { "Other device" }
                        }
                        td data-label="Last active" {
                            time datetime=(s.last_touched_at.to_rfc3339()) data-fmt="datetime" { (s.last_touched_at.format("%Y-%m-%d %H:%M UTC")) }
                        }
                        td data-label="Signed in" {
                            time datetime=(s.created_at.to_rfc3339()) data-fmt="datetime" { (s.created_at.format("%Y-%m-%d %H:%M UTC")) }
                        }
                    }
                }
            }
        }
        p style="margin-top: var(--sp-4);" {
            a href="/settings/logout-others" class="btn btn-primary"
                data-confirm="Log out of all other devices? This device stays signed in." { "Log out all other devices" }
        }
    }
}
