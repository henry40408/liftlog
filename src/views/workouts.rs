use chrono::NaiveDate;
use maud::{Markup, PreEscaped, html};

use super::layout::{nav, page};
use crate::middleware::AuthUser;
use crate::models::{WorkoutLog, WorkoutLogWithExercise, WorkoutSession};

fn error_box(error: Option<&str>) -> Markup {
    html! {
        @if let Some(err) = error { div class="error" { (err) } }
    }
}

pub struct WorkoutsListView<'a> {
    pub user: &'a AuthUser,
    pub workouts: &'a [WorkoutSession],
    pub page: i64,
    pub total_pages: i64,
}

pub fn list(view: &WorkoutsListView) -> Markup {
    let body = html! {
        (nav(view.user))
        main {
            div class="page-header" { h1 { "Workouts" } }
            p class="mb-6" { a href="/workouts/new" class="btn btn-primary" { "+ New Workout" } }
            @if view.workouts.is_empty() {
                div class="empty-state" { p class="muted" { "No workouts yet. Time to hit the iron." } }
            } @else {
                div class="workout-list" {
                    @for w in view.workouts {
                        div class="workout-item" {
                            a href={"/workouts/" (w.id)} { (w.date) }
                            span class="notes" {
                                @if let Some(n) = &w.notes { (n) } @else { (PreEscaped("&mdash;")) }
                            }
                        }
                    }
                }
                @if view.total_pages > 1 { (pagination(view.page, view.total_pages)) }
            }
        }
    };
    page("Workouts - LiftLog", body)
}

fn pagination(page: i64, total_pages: i64) -> Markup {
    html! {
        div class="pagination" {
            span { "Page " (page) " / " (total_pages) }
            @if page > 1 {
                a href={"/workouts?page=" (page - 1)} { (PreEscaped("&larr;")) " Previous" }
            }
            @if page < total_pages {
                a href={"/workouts?page=" (page + 1)} { "Next " (PreEscaped("&rarr;")) }
            }
        }
    }
}

pub struct NewWorkoutView<'a> {
    pub user: &'a AuthUser,
    pub today: NaiveDate,
    /// Without a saved timezone the server only knows UTC, so the page falls
    /// back to the browser's date.
    pub use_browser_today: bool,
    pub error: Option<&'a str>,
}

/// Swaps the server's UTC "today" for the browser's, unless the user changed it.
const BROWSER_TODAY_JS: &str = "
/* No saved timezone: swap the server's UTC \"today\" for the browser's, unless
   the user changed it (e.g. a bfcache restore). */
(function () {
    var dateEl = document.getElementById('date');
    if (dateEl.value !== dateEl.dataset.serverToday) {
        return;
    }
    var browserToday = window.LiftLog.formatLocalDate(new Date().toISOString());
    if (browserToday) {
        dateEl.value = browserToday;
    }
})();
";

pub fn new(view: &NewWorkoutView) -> Markup {
    let body = html! {
        (nav(view.user))
        main {
            div class="page-header" { h1 { "New Workout" } }
            (error_box(view.error))
            form method="post" action="/workouts" {
                div class="form-group" {
                    label for="date" { "Date" }
                    input type="date" id="date" name="date" value=(view.today) data-server-today=(view.today) required;
                }
                div class="form-group" {
                    label for="notes" { "Notes (optional)" }
                    textarea id="notes" name="notes" rows="3" {}
                }
                button type="submit" { "Create Workout" }
            }
            a href="/workouts" class="back-link" { (PreEscaped("&larr;")) " Back to Workouts" }
        }
        @if view.use_browser_today {
            script { (PreEscaped(BROWSER_TODAY_JS)) }
        }
    };
    page("New Workout - LiftLog", body)
}

pub fn edit(user: &AuthUser, workout: &WorkoutSession, error: Option<&str>) -> Markup {
    let body = html! {
        (nav(user))
        main {
            div class="page-header" { h1 { "Edit Workout" } }
            (error_box(error))
            form method="post" action={"/workouts/" (workout.id)} {
                div class="form-group" {
                    label for="date" { "Date" }
                    input type="date" id="date" name="date" value=(workout.date) required;
                }
                div class="form-group" {
                    label for="notes" { "Notes (optional)" }
                    textarea id="notes" name="notes" rows="3" { @if let Some(n) = &workout.notes { (n) } }
                }
                button type="submit" { "Save Changes" }
            }
            a href={"/workouts/" (workout.id)} class="back-link" { (PreEscaped("&larr;")) " Back to Workout" }
        }
    };
    page("Edit Workout - LiftLog", body)
}

pub struct EditLogView<'a> {
    pub user: &'a AuthUser,
    pub workout: &'a WorkoutSession,
    pub log: &'a WorkoutLog,
    pub exercise_name: &'a str,
    pub error: Option<&'a str>,
}

pub fn edit_log(view: &EditLogView) -> Markup {
    let (workout, log) = (view.workout, view.log);
    let body = html! {
        (nav(view.user))
        main {
            div class="page-header" { h1 { "Edit Set" } }
            (error_box(view.error))
            form method="post" action={"/workouts/" (workout.id) "/logs/" (log.id)} {
                div class="form-group" {
                    label { "Exercise" }
                    input type="text" value=(view.exercise_name) disabled style="opacity: 0.6;";
                }
                div class="form-group" {
                    label { "Set Number" }
                    input type="number" inputmode="numeric" value=(log.set_number) disabled style="opacity: 0.6;";
                }
                div class="form-group" {
                    label for="weight" { "Weight" }
                    input type="number" inputmode="decimal" id="weight" name="weight" step="0.25" min="0" value=(log.weight) required;
                }
                div class="form-group" {
                    label for="reps" { "Reps" }
                    input type="number" inputmode="numeric" id="reps" name="reps" min="1" value=(log.reps) required;
                }
                div class="form-group" {
                    label for="rpe" { "RPE (1-10, optional)" }
                    input type="number" inputmode="numeric" id="rpe" name="rpe" min="1" max="10" value=[log.rpe];
                }
                button type="submit" { "Save Changes" }
            }
            a href={"/workouts/" (workout.id)} class="back-link" { (PreEscaped("&larr;")) " Back to Workout" }
        }
    };
    page("Edit Set - LiftLog", body)
}

pub fn shared(
    workout: &WorkoutSession,
    logs: &[WorkoutLogWithExercise],
    owner_username: &str,
) -> Markup {
    let body = html! {
        main style="max-width: 960px; margin: 0 auto; padding: var(--sp-8) var(--sp-6);" {
            div class="page-header" {
                h1 { (workout.date) }
                div class="subtitle" { "Shared by " strong style="color: var(--text-primary);" { (owner_username) } }
            }
            @if let Some(notes) = &workout.notes {
                p class="muted" style="margin-bottom: var(--sp-6);" { em { (notes) } }
            }
            h2 { "Sets" }
            @if logs.is_empty() {
                p class="muted" { "No sets recorded." }
            } @else {
                div class="sets-list" {
                    div class="sets-header" {
                        div { "Exercise" } div { "Set" } div { "Weight" } div { "Reps" } div { "RPE" } div {}
                    }
                    @for log in logs { (shared_row(log)) }
                }
            }
            div class="mt-8" {
                p class="muted text-sm" { "This is a shared workout from " a href="/" { "LiftLog" } "." }
            }
        }
    };
    page(&format!("Shared Workout {} - LiftLog", workout.date), body)
}

fn shared_row(log: &WorkoutLogWithExercise) -> Markup {
    html! {
        div class="set-row" {
            div class="set-cell set-cell-exercise" style="color: var(--text-primary);" { (log.exercise_name) }
            div class="set-cell set-cell-set" { (log.set_number) }
            div class="set-cell set-cell-weight" { (log.weight) }
            div class="set-cell set-cell-reps" { (log.reps) }
            div class="set-cell set-cell-rpe" { @if let Some(r) = log.rpe { (r) } @else { "-" } }
            div class="set-cell set-cell-pr" {}
        }
    }
}
