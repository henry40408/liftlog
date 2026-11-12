use maud::{Markup, PreEscaped, html};

use super::layout::{nav, page};
use crate::middleware::AuthUser;
use crate::models::UserListItem;

fn error_box(error: Option<&str>) -> Markup {
    html! {
        @if let Some(err) = error { div class="error" { (err) } }
    }
}

pub fn login(error: Option<&str>) -> Markup {
    let body = html! {
        main {
            div class="auth-container" {
                div class="auth-logo" { "Lift" span { "Log" } }
                div class="auth-subtitle" { "Track your iron journey" }
                (error_box(error))
                form method="post" action="/auth/login" {
                    div class="form-group" {
                        label for="username" { "Username" }
                        input type="text" id="username" name="username" required autofocus autocomplete="username";
                    }
                    div class="form-group" {
                        label for="password" { "Password" }
                        input type="password" id="password" name="password" required autocomplete="current-password";
                    }
                    button type="submit" { "Login" }
                }
            }
        }
    };
    page("Login - LiftLog", body)
}

pub fn setup(error: Option<&str>) -> Markup {
    let body = html! {
        main {
            div class="auth-container" {
                div class="auth-logo" { "Lift" span { "Log" } }
                div class="auth-subtitle" { "Create the first user account to get started" }
                (error_box(error))
                form method="post" action="/auth/setup" {
                    div class="form-group" {
                        label for="setup_token" { "Setup token" }
                        input type="text" id="setup_token" name="setup_token" required autofocus autocomplete="off" autocapitalize="characters" spellcheck="false" placeholder="XXXXX-XXXXX";
                        small { "Find it on the server's stderr (`docker logs` shows it), on the line starting with \"Setup token\". It is shown only until the first account exists." }
                    }
                    div class="form-group" {
                        label for="username" { "Username" }
                        input type="text" id="username" name="username" required;
                    }
                    div class="form-group" {
                        label for="password" { "Password (min 8 characters)" }
                        input type="password" id="password" name="password" required minlength="12" maxlength="128";
                    }
                    button type="submit" { "Create Account" }
                }
            }
        }
    };
    page("Setup - LiftLog", body)
}

pub fn new_user(user: &AuthUser, error: Option<&str>) -> Markup {
    let body = html! {
        (nav(user))
        main {
            div class="page-header" { h1 { "Add New User" } }
            (error_box(error))
            form method="post" action="/users/new" {
                div class="form-group" {
                    label for="username" { "Username" }
                    input type="text" id="username" name="username" required autofocus;
                }
                div class="form-group" {
                    label for="password" { "Password (min 8 characters)" }
                    input type="password" id="password" name="password" required minlength="12" maxlength="128";
                }
                button type="submit" { "Create User" }
            }
            a href="/users" class="back-link" { (PreEscaped("&larr;")) " Back to Users" }
        }
    };
    page("New User - LiftLog", body)
}

pub struct ConfirmActionView<'a> {
    pub user: &'a AuthUser,
    pub action_label: &'a str,
    pub consequence: &'a str,
    pub form_action: &'a str,
    pub error: Option<&'a str>,
}

/// Promote/delete-user confirmation: also re-checks the admin's password.
pub fn confirm_action(view: &ConfirmActionView) -> Markup {
    let body = html! {
        (nav(view.user))
        main {
            div class="page-header" { h1 { (view.action_label) } }
            (error_box(view.error))
            p class="mb-4" { (view.consequence) }
            form method="post" action=(view.form_action) {
                div class="form-group" {
                    label for="current_password" { "Confirm your password to continue" }
                    input type="password" id="current_password" name="current_password" required autocomplete="current-password" autofocus;
                }
                button type="submit" class="btn btn-danger" { (view.action_label) }
                a href="/users" class="btn btn-ghost" { "Cancel" }
            }
        }
    };
    page(&format!("{} - LiftLog", view.action_label), body)
}

pub fn users_list(user: &AuthUser, users: &[UserListItem]) -> Markup {
    let is_admin = user.is_admin();
    let body = html! {
        (nav(user))
        main {
            div class="page-header" { h1 { "Users" } }
            @if is_admin {
                p class="mb-4" { a href="/users/new" class="btn btn-primary" { "+ Add New User" } }
            }
            table class="data-table" {
                thead { tr {
                    th { "Username" }
                    th { "Role" }
                    th { "Created" }
                    @if is_admin { th { "Actions" } }
                } }
                tbody {
                    @for u in users { (user_row(user, u)) }
                }
            }
        }
    };
    page("Users - LiftLog", body)
}

fn user_row(viewer: &AuthUser, u: &UserListItem) -> Markup {
    html! {
        tr {
            td data-label="Username" style="color: var(--text-primary);" { (u.username) }
            td data-label="Role" {
                @if u.role.is_admin() {
                    span style="color: var(--gold);" { (u.role.as_str()) }
                } @else { (u.role.as_str()) }
            }
            td data-label="Created" {
                time datetime=(u.created_at.to_rfc3339()) data-fmt="datetime" { (u.created_at.format("%Y-%m-%d %H:%M UTC")) }
            }
            @if viewer.is_admin() {
                td data-label="Actions" {
                    @if u.id == viewer.id {
                        span class="muted" { "(you)" }
                    } @else {
                        // Deliberately no data-confirm: these pages re-check the admin's password.
                        div class="actions" {
                            @if !u.role.is_admin() {
                                a href={"/users/" (u.id) "/promote"} class="btn btn-ghost btn-sm" { "Promote" }
                            }
                            a href={"/users/" (u.id) "/delete"} class="btn btn-danger btn-sm" { "Delete" }
                        }
                    }
                }
            }
        }
    }
}
