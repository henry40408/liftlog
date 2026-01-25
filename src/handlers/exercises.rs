use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};

use crate::error::{AppError, Result};
use crate::middleware::AuthUser;
use crate::models::{exercise::CATEGORIES, CreateExercise, Exercise};
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
    categories: &'static [crate::models::exercise::ExerciseCategory],
}

#[derive(Template)]
#[template(path = "exercises/new.html")]
struct NewExerciseTemplate {
    user: AuthUser,
    categories: &'static [crate::models::exercise::ExerciseCategory],
    error: Option<String>,
}

pub async fn list(
    State(state): State<ExercisesState>,
    auth_user: AuthUser,
) -> Result<Response> {
    let exercises = state.exercise_repo.find_available_for_user(&auth_user.id).await?;

    let template = ExercisesListTemplate {
        user: auth_user,
        exercises,
        categories: CATEGORIES,
    };

    Ok(Html(template.render().map_err(|e| AppError::Internal(e.to_string()))?).into_response())
}

pub async fn new_page(auth_user: AuthUser) -> Result<Response> {
    let template = NewExerciseTemplate {
        user: auth_user,
        categories: CATEGORIES,
        error: None,
    };

    Ok(Html(template.render().map_err(|e| AppError::Internal(e.to_string()))?).into_response())
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
        return Ok(Html(template.render().map_err(|e| AppError::Internal(e.to_string()))?).into_response());
    }

    state
        .exercise_repo
        .create(
            &form.name,
            &form.category,
            &form.muscle_group,
            form.equipment.as_deref(),
            &auth_user.id,
        )
        .await?;

    Ok(Redirect::to("/exercises").into_response())
}
