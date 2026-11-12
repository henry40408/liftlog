use axum::response::Html;
use maud::{DOCTYPE, Markup, PreEscaped, html};

use crate::middleware::AuthUser;

const BASE_CSS: &str = include_str!("base.css");
const BASE_JS: &str = include_str!("base.js");

/// The document shell shared by every page. `title` is the full `<title>`.
pub fn page(title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="zh-TW" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                link rel="icon" type="image/svg+xml" href="/favicon.svg";
                link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png";
                title { (title) }
                link rel="preconnect" href="https://fonts.bunny.net";
                link href="https://fonts.bunny.net/css2?family=Chakra+Petch:wght@400;600;700&family=Overpass+Mono:wght@400;500;600;700&display=swap" rel="stylesheet";
                style { (PreEscaped(BASE_CSS)) }
                script { (PreEscaped(BASE_JS)) }
            }
            body {
                div class="app-accent-bar" {}
                div class="app" { (body) }
            }
        }
    }
}

pub fn nav(user: &AuthUser) -> Markup {
    html! {
        nav class="nav-bar" {
            a href="/" class="nav-logo" { "Lift" span { "Log" } }
            " "
            input type="checkbox" id="nav-toggle" class="nav-toggle-checkbox";
            " "
            label for="nav-toggle" class="nav-toggle-label" { " " span class="nav-toggle-icon" {} " " }
            ul class="nav-links" {
                li { a href="/" { "Dashboard" } }
                li { a href="/workouts" { "Workouts" } }
                li { a href="/exercises" { "Exercises" } }
                li { a href="/stats" { "Stats" } }
                li { a href="/settings" { "Settings" } }
                li { a href="/users" { "Users" } }
            }
            div class="nav-user" {
                span class="username" {
                    (user.username)
                    @if user.is_admin() { " " span class="admin-badge" { "Admin" } }
                }
                form action="/auth/logout" method="post" style="display:inline;" {
                    button type="submit" class="sign-out-btn" { "Sign Out" }
                }
            }
        }
    }
}

pub fn html_page(markup: Markup) -> Html<String> {
    Html(markup.into_string())
}
