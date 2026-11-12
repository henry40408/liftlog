use chrono::{DateTime, Utc};
use maud::{Markup, PreEscaped, html};

use super::layout::{nav, page};
use crate::middleware::AuthUser;
use crate::models::exercise::ExerciseCategory;
use crate::models::{Exercise, LastExerciseWeight, WorkoutLogWithExercise, WorkoutSession};

/// Add Set form values from `?prefill=<log_id>` (no-JS Clone).
pub struct PrefillSet {
    pub exercise_id: String,
    pub weight: f64,
    pub reps: i32,
    pub rpe: Option<i32>,
}

/// One row of the `<noscript>` last-weights list.
pub struct LastWeightRow {
    pub exercise_name: String,
    pub weight: f64,
    pub rpe: Option<i32>,
    pub logged_at: DateTime<Utc>,
}

pub struct ShareInfo<'a> {
    pub url: &'a str,
    pub expires_at: Option<DateTime<Utc>>,
}

pub struct ShowWorkoutView<'a> {
    pub user: &'a AuthUser,
    pub workout: &'a WorkoutSession,
    pub logs: &'a [WorkoutLogWithExercise],
    pub exercises: &'a [Exercise],
    pub categories: &'a [ExerciseCategory],
    pub exercise_last_weights: &'a [LastExerciseWeight],
    pub last_weight_rows: &'a [LastWeightRow],
    /// Datalist values; see `weight_suggestions` in the handler.
    pub weight_suggestions: &'a [f64],
    pub rep_suggestions: &'a [i32],
    pub prefill: Option<&'a PrefillSet>,
    pub share: Option<ShareInfo<'a>>,
    pub error: Option<&'a str>,
}

/// The static half of the page script; the per-exercise data is emitted before it.
const SHOW_JS: &str = include_str!("workout_show.js");

pub fn show(view: &ShowWorkoutView) -> Markup {
    let workout = view.workout;
    let body = html! {
        (nav(view.user))
        main {
            div class="page-header" {
                h1 { (workout.date) }
                @if let Some(notes) = &workout.notes { div class="subtitle" { em { (notes) } } }
            }
            div class="actions mb-6" {
                a href={"/workouts/" (workout.id) "/edit"} class="btn btn-ghost btn-sm" { "Edit" }
                " "
                a href={"/workouts/" (workout.id) "/delete"} class="btn btn-danger btn-sm"
                    data-confirm="Delete this workout? Every set recorded in it is deleted too, and this cannot be undone." { "Delete" }
                @if view.share.is_none() {
                    " "
                    (share_form(workout))
                }
            }
            @if let Some(share) = &view.share { (share_info(workout, share)) }
            h3 id="add-set" { "Add Set" }
            div class="card card-accent" style="margin: var(--sp-4) 0;" {
                (add_set_form(view))
                // No-JS stand-in for the last-weight hint, which needs the <select> change event.
                noscript { (last_weights_table(view.last_weight_rows)) }
            }
            h2 { "Sets" }
            @if let Some(err) = view.error { div class="error" { (err) } }
            @if view.logs.is_empty() {
                p class="muted" { "No sets recorded yet." }
            } @else {
                div class="sets-list" {
                    div class="sets-header" {
                        div { "Exercise" } div { "Set" } div { "Weight" } div { "Reps" } div { "RPE" } div {}
                    }
                    @for log in view.logs { (set_row(workout, log)) }
                }
            }
            a href="/workouts" class="back-link" { (PreEscaped("&larr;")) " Back to Workouts" }
        }
        script {
            (PreEscaped("\nvar exerciseLastWeights = {};\n"))
            @for entry in view.exercise_last_weights { (last_weight_entry(entry)) }
            (PreEscaped("\n"))
            (PreEscaped(SHOW_JS))
        }
    };
    page(&format!("Workout {} - LiftLog", workout.date), body)
}

fn share_form(workout: &WorkoutSession) -> Markup {
    html! {
        form action={"/workouts/" (workout.id) "/share"} method="post" class="share-form" {
            select id="expires_in_days" name="expires_in_days" aria-label="Share link expiry" {
                option value="" selected { "Never expires" }
                option value="7" { "7 days" }
                option value="30" { "30 days" }
                option value="90" { "90 days" }
            }
            " "
            button type="submit" class="btn btn-ghost btn-sm" { "Share" }
        }
    }
}

fn share_info(workout: &WorkoutSession, share: &ShareInfo) -> Markup {
    html! {
        div class="share-info" {
            p style="margin: 0 0 var(--sp-3) 0;" {
                "Share link: " a href=(share.url) target="_blank" { (share.url) }
                " "
                // Needs the clipboard API: hidden until base.html reveals it.
                button type="button" class="btn btn-sm btn-inline" hidden data-requires-js
                    data-copy-url=(share.url)
                    style="background:var(--accent);color:var(--text-inverse);border-color:var(--accent);" { "Copy" }
            }
            @if let Some(expires_at) = share.expires_at {
                p class="muted" style="margin: 0 0 var(--sp-3) 0;" {
                    "Expires "
                    time datetime=(expires_at.to_rfc3339()) data-fmt="datetime" { (expires_at.format("%Y-%m-%d %H:%M UTC")) }
                }
            } @else {
                p class="muted" style="margin: 0 0 var(--sp-3) 0;" { "Never expires" }
            }
            a href={"/workouts/" (workout.id) "/revoke-share"} class="btn btn-danger btn-sm"
                data-confirm="Revoke the share link? Anyone who already has it loses access, and sharing again produces a different link." { "Revoke Share" }
        }
    }
}

fn add_set_form(view: &ShowWorkoutView) -> Markup {
    let prefill = view.prefill;
    html! {
        form method="post" action={"/workouts/" (view.workout.id) "/logs"} {
            div class="form-group" {
                label for="exercise_id" { "Exercise" }
                select id="exercise_id" name="exercise_id" required {
                    option value="" { "-- Select --" }
                    @for cat in view.categories {
                        optgroup label=(cat.display_name) {
                            @for ex in view.exercises.iter().filter(|ex| ex.category == cat.name) {
                                option value=(ex.id) selected[prefill.is_some_and(|p| p.exercise_id == ex.id)] { (ex.name) }
                            }
                        }
                    }
                }
                div id="exercise-last-weight-info" class="pr-info" {}
            }
            div class="form-group" {
                label for="weight" { "Weight" }
                // Datalist, not <select>: suggestions, any weight still typeable.
                input type="number" inputmode="decimal" id="weight" name="weight" step="0.25" min="0" list="weight-list" autocomplete="off" required value=[prefill.map(|p| p.weight)];
                @if !view.weight_suggestions.is_empty() {
                    datalist id="weight-list" {
                        @for w in view.weight_suggestions { option value=(w) {} }
                    }
                }
            }
            div class="form-group" {
                label for="reps" { "Reps" }
                // Fixed list: `models::workout_log::REP_SCHEMES`.
                input type="number" inputmode="numeric" id="reps" name="reps" min="1" list="reps-list" autocomplete="off" required value=[prefill.map(|p| p.reps)];
                datalist id="reps-list" {
                    @for r in view.rep_suggestions { option value=(r) {} }
                }
            }
            div class="form-group" {
                label for="rpe" { "RPE (1-10, optional)" }
                input type="number" inputmode="numeric" id="rpe" name="rpe" min="1" max="10" value=[prefill.and_then(|p| p.rpe)];
            }
            button type="submit" { "Add Set" }
        }
    }
}

fn last_weights_table(rows: &[LastWeightRow]) -> Markup {
    html! {
        @if !rows.is_empty() {
            h4 style="margin: var(--sp-5) 0 var(--sp-2) 0;" { "Last time" }
            table class="data-table" {
                thead { tr { th { "Exercise" } th { "Weight" } th { "RPE" } th { "Logged" } } }
                tbody {
                    @for row in rows {
                        tr {
                            td data-label="Exercise" { (row.exercise_name) }
                            td data-label="Weight" { (row.weight) " kg" }
                            td data-label="RPE" { @if let Some(r) = row.rpe { (r) } @else { "-" } }
                            td data-label="Logged" {
                                time datetime=(row.logged_at.to_rfc3339()) data-fmt="date" { (row.logged_at.format("%Y-%m-%d")) }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn set_row(workout: &WorkoutSession, log: &WorkoutLogWithExercise) -> Markup {
    html! {
        div class="set-row" {
            div class="set-cell set-cell-exercise" { a href={"/stats/exercise/" (log.exercise_id)} { (log.exercise_name) } }
            div class="set-cell set-cell-set" { (log.set_number) }
            div class="set-cell set-cell-weight" { (log.weight) }
            div class="set-cell set-cell-reps" { (log.reps) }
            div class="set-cell set-cell-rpe" { @if let Some(r) = log.rpe { (r) } @else { "-" } }
            div class="set-cell set-cell-pr" {
                @if log.is_pr {
                    span class="pr-badge" title="All-time best for this exercise" { "PR" }
                } @else if log.is_recent_pr {
                    span class="pr-badge pr-badge-recent" title="Best for this exercise in the last month" { "PR 1M" }
                }
            }
            div class="set-row-actions" {
                a href={"/workouts/" (workout.id) "/logs/" (log.id) "/edit"} class="btn btn-ghost btn-sm" { "Edit" }
                " "
                // href works without scripts; the handler below fills in place.
                a href={"?prefill=" (log.id) "#add-set"} class="btn btn-ghost btn-sm"
                    data-clone-exercise=(log.exercise_id)
                    data-clone-weight=(log.weight)
                    data-clone-reps=(log.reps)
                    data-clone-rpe=(log.rpe.map_or_else(String::new, |r| r.to_string())) { "Clone" }
                " "
                a href={"/workouts/" (workout.id) "/logs/" (log.id) "/delete"}
                    class="btn btn-danger btn-sm" aria-label="Delete set"
                    data-confirm={"Delete set " (log.set_number) " of " (log.exercise_name) "?"} { (PreEscaped("&times;")) }
            }
        }
    }
}

/// One `exerciseLastWeights[...]` assignment of the page script.
fn last_weight_entry(entry: &LastExerciseWeight) -> Markup {
    let rpe = entry
        .rpe
        .map_or_else(|| "null".to_string(), |r| r.to_string());
    html! {
        (PreEscaped("\nexerciseLastWeights[\""))
        (entry.exercise_id)
        (PreEscaped("\"] = {\n    weight: "))
        (entry.weight)
        (PreEscaped(",\n    rpe: "))
        (rpe)
        (PreEscaped(",\n    loggedAt: \""))
        (entry.logged_at.to_rfc3339())
        (PreEscaped("\"\n};\n"))
    }
}
