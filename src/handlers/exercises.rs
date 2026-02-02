use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};

use crate::error::{AppError, Result};
use crate::middleware::AuthUser;
use crate::models::exercise::{ExerciseCategory, CATEGORIES};
use crate::models::{CreateExercise, Exercise, UpdateExercise};
use crate::repositories::ExerciseRepository;

#[derive(Clone)]
pub struct ExercisesState {
    pub exercise_repo: ExerciseRepository,
}

#[derive(Template)]
#[template(path = "exercises/list.html")]
struct ExercisesListTemplate {
    user: AuthUser,
    exercises: Vec<Exercise>,
    categories: &'static [ExerciseCategory],
}

#[derive(Template)]
#[template(path = "exercises/new.html")]
struct NewExerciseTemplate {
    user: AuthUser,
    categories: &'static [ExerciseCategory],
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "exercises/edit.html")]
struct EditExerciseTemplate {
    user: AuthUser,
    exercise: Exercise,
    categories: &'static [ExerciseCategory],
    error: Option<String>,
}

pub async fn list(State(state): State<ExercisesState>, auth_user: AuthUser) -> Result<Response> {
    let exercises = state
        .exercise_repo
        .find_available_for_user(&auth_user.id)
        .await?;

    let template = ExercisesListTemplate {
        user: auth_user,
        exercises,
        categories: CATEGORIES,
    };

    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn new_page(auth_user: AuthUser) -> Result<Response> {
    let template = NewExerciseTemplate {
        user: auth_user,
        categories: CATEGORIES,
        error: None,
    };

    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn create(
    State(state): State<ExercisesState>,
    auth_user: AuthUser,
    Form(form): Form<CreateExercise>,
) -> Result<Response> {
    if form.name.trim().is_empty() {
        let template = NewExerciseTemplate {
            user: auth_user,
            categories: CATEGORIES,
            error: Some("Exercise name is required".to_string()),
        };
        return Ok(Html(
            template
                .render()
                .map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .into_response());
    }

    state
        .exercise_repo
        .create(&form.name, &form.category, &auth_user.id)
        .await?;

    Ok(Redirect::to("/exercises").into_response())
}

pub async fn edit_page(
    State(state): State<ExercisesState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    let exercise = state
        .exercise_repo
        .find_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Exercise not found".to_string()))?;

    if exercise.user_id != auth_user.id {
        return Err(AppError::Forbidden(
            "You can only edit your own exercises".to_string(),
        ));
    }

    let template = EditExerciseTemplate {
        user: auth_user,
        exercise,
        categories: CATEGORIES,
        error: None,
    };

    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn update(
    State(state): State<ExercisesState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
    Form(form): Form<UpdateExercise>,
) -> Result<Response> {
    let exercise = state
        .exercise_repo
        .find_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Exercise not found".to_string()))?;

    if exercise.user_id != auth_user.id {
        return Err(AppError::Forbidden(
            "You can only edit your own exercises".to_string(),
        ));
    }

    if form.name.trim().is_empty() {
        let template = EditExerciseTemplate {
            user: auth_user,
            exercise,
            categories: CATEGORIES,
            error: Some("Exercise name is required".to_string()),
        };
        return Ok(Html(
            template
                .render()
                .map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .into_response());
    }

    state
        .exercise_repo
        .update(&id, &auth_user.id, &form.name, &form.category)
        .await?;

    Ok(Redirect::to("/exercises").into_response())
}

pub async fn delete(
    State(state): State<ExercisesState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    let exercise = state
        .exercise_repo
        .find_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Exercise not found".to_string()))?;

    if exercise.user_id != auth_user.id {
        return Err(AppError::Forbidden(
            "You can only delete your own exercises".to_string(),
        ));
    }

    state.exercise_repo.delete(&id, &auth_user.id).await?;

    Ok(Redirect::to("/exercises").into_response())
}
