use maud::{Markup, PreEscaped, html};

use super::layout::{nav, page};
use crate::middleware::AuthUser;
use crate::models::WorkoutSession;

pub struct DashboardView<'a> {
    pub user: &'a AuthUser,
    pub workouts_this_week: i64,
    pub workouts_this_month: i64,
    pub total_volume: f64,
    pub recent_workouts: &'a [WorkoutSession],
    pub timezone_unset: bool,
}

pub fn dashboard(view: &DashboardView) -> Markup {
    let body = html! {
        (nav(view.user))
        main {
            div class="page-header" {
                h1 { "Dashboard" }
                div class="subtitle" { "Your training overview" }
            }
            div class="actions actions-lead" {
                a href="/workouts/new" class="btn btn-primary" { "+ New Workout" }
                a href="/stats/prs" class="btn btn-ghost" { "View PRs" }
            }
            @if view.timezone_unset {
                div class="alert alert-warning" role="status" {
                    span class="alert-text" { "Your timezone is not set, so weekly and monthly stats use UTC dates." }
                    a href="/settings#timezone" class="btn btn-primary btn-sm" { "Set timezone" }
                }
            }
            div class="stats-grid" {
                (stat_card(view.workouts_this_week, "This Week"))
                (stat_card(view.workouts_this_month, "This Month"))
                (stat_card(view.total_volume, "Volume (Week)"))
            }
            h2 { "Recent Workouts" }
            @if view.recent_workouts.is_empty() {
                div class="empty-state" {
                    p class="muted" { "No workouts yet." }
                    a href="/workouts/new" class="btn btn-primary" { "Start Your First Workout" }
                }
            } @else {
                div class="workout-list" {
                    @for w in view.recent_workouts {
                        div class="workout-item" {
                            a href={"/workouts/" (w.id)} { (w.date) }
                            span class="notes" {
                                @if let Some(n) = &w.notes { (n) } @else { (PreEscaped("&mdash;")) }
                            }
                        }
                    }
                }
                p class="mt-4" { a href="/workouts" { "View All Workouts " (PreEscaped("&rarr;")) } }
            }
        }
    };
    page("Dashboard - LiftLog", body)
}

fn stat_card(value: impl std::fmt::Display, label: &str) -> Markup {
    html! {
        div class="stat-card" {
            div class="stat-value" { (value) }
            div class="stat-label" { (label) }
        }
    }
}
