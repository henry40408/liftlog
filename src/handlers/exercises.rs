use axum::{
    Form,
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
};

use crate::error::Result;
use crate::handlers::confirm;
use crate::middleware::AuthUser;
use crate::models::exercise::CATEGORIES;
use crate::models::{CreateExercise, UpdateExercise};
use crate::state::AppState;
use crate::views::exercises;

pub async fn list(State(state): State<AppState>, auth_user: AuthUser) -> Result<Response> {
    let exercises = state
        .exercise_repo
        .find_available_for_user(&auth_user.id)
        .await?;

    Ok(exercises::list(&auth_user, &exercises, CATEGORIES).into_response())
}

pub async fn new_page(auth_user: AuthUser) -> Result<Response> {
    Ok(exercises::new(&auth_user, CATEGORIES, None).into_response())
}

pub async fn create(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Form(form): Form<CreateExercise>,
) -> Result<Response> {
    if form.name.trim().is_empty() {
        let page = exercises::new(&auth_user, CATEGORIES, Some("Exercise name is required"));
        return Ok(page.into_response());
    }

    state
        .exercise_repo
        .create(&form.name, &form.category, &auth_user.id)
        .await?;

    Ok(Redirect::to("/exercises").into_response())
}

pub async fn edit_page(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    let exercise = state.exercise_repo.find_owned(&id, &auth_user.id).await?;

    Ok(exercises::edit(&auth_user, &exercise, CATEGORIES, None).into_response())
}

pub async fn update(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
    Form(form): Form<UpdateExercise>,
) -> Result<Response> {
    let exercise = state.exercise_repo.find_owned(&id, &auth_user.id).await?;

    if form.name.trim().is_empty() {
        let page = exercises::edit(
            &auth_user,
            &exercise,
            CATEGORIES,
            Some("Exercise name is required"),
        );
        return Ok(page.into_response());
    }

    state
        .exercise_repo
        .update(&id, &auth_user.id, &form.name, &form.category)
        .await?;

    Ok(Redirect::to("/exercises").into_response())
}

/// Interstitial for `delete`. `delete` itself still enforces the
/// referenced-exercise check (`ON DELETE RESTRICT`).
pub async fn confirm_delete(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    let exercise = state.exercise_repo.find_owned(&id, &auth_user.id).await?;

    Ok(confirm::page(
        auth_user,
        "Delete exercise",
        format!(
            "{name} will be permanently deleted. If any workout still logs it, the deletion will be refused instead.",
            name = exercise.name
        ),
        format!("/exercises/{id}/delete"),
        "/exercises".to_string(),
    ))
}

pub async fn delete(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    state.exercise_repo.find_owned(&id, &auth_user.id).await?;

    state.exercise_repo.delete(&id, &auth_user.id).await?;

    Ok(Redirect::to("/exercises").into_response())
}
