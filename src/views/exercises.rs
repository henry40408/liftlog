use maud::{Markup, PreEscaped, html};

use super::layout::{nav, page};
use crate::middleware::AuthUser;
use crate::models::Exercise;
use crate::models::exercise::ExerciseCategory;

pub fn list(user: &AuthUser, exercises: &[Exercise], categories: &[ExerciseCategory]) -> Markup {
    let body = html! {
        (nav(user))
        main {
            div class="page-header" { h1 { "Exercises" } }
            p class="mb-6" { a href="/exercises/new" class="btn btn-primary" { "+ Add Exercise" } }
            @for cat in categories {
                div class="category-section" {
                    div class="category-title" { (cat.display_name) }
                    @for ex in exercises.iter().filter(|ex| ex.category == cat.name) {
                        div class="exercise-item" {
                            a href={"/stats/exercise/" (ex.id)} { (ex.name) }
                            div class="actions" {
                                a href={"/exercises/" (ex.id) "/edit"} class="btn btn-ghost btn-sm" { "Edit" }
                                a href={"/exercises/" (ex.id) "/delete"} class="btn btn-danger btn-sm"
                                    data-confirm={"Delete " (ex.name) "?"} { "Delete" }
                            }
                        }
                    }
                }
            }
        }
    };
    page("Exercises - LiftLog", body)
}

pub fn new(user: &AuthUser, categories: &[ExerciseCategory], error: Option<&str>) -> Markup {
    let body = html! {
        (nav(user))
        main {
            div class="page-header" { h1 { "Add Exercise" } }
            @if let Some(err) = error { div class="error" { (err) } }
            form method="post" action="/exercises" {
                div class="form-group" {
                    label for="name" { "Name" }
                    input type="text" id="name" name="name" required autofocus;
                }
                div class="form-group" {
                    label for="category" { "Category" }
                    select id="category" name="category" required {
                        @for cat in categories { option value=(cat.name) { (cat.display_name) } }
                    }
                }
                button type="submit" { "Add Exercise" }
            }
            a href="/exercises" class="back-link" { (PreEscaped("&larr;")) " Back to Exercises" }
        }
    };
    page("New Exercise - LiftLog", body)
}

pub fn edit(
    user: &AuthUser,
    exercise: &Exercise,
    categories: &[ExerciseCategory],
    error: Option<&str>,
) -> Markup {
    let body = html! {
        (nav(user))
        main {
            div class="page-header" { h1 { "Edit Exercise" } }
            @if let Some(err) = error { div class="error" { (err) } }
            form method="post" action={"/exercises/" (exercise.id)} {
                div class="form-group" {
                    label for="name" { "Name" }
                    input type="text" id="name" name="name" value=(exercise.name) required autofocus;
                }
                div class="form-group" {
                    label for="category" { "Category" }
                    select id="category" name="category" required {
                        @for cat in categories {
                            option value=(cat.name) selected[cat.name == exercise.category] { (cat.display_name) }
                        }
                    }
                }
                button type="submit" { "Save Changes" }
            }
            a href="/exercises" class="back-link" { (PreEscaped("&larr;")) " Back to Exercises" }
        }
    };
    page("Edit Exercise - LiftLog", body)
}
