use askama::Template;
use axum::{
    extract::{Path, Query, State},
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use chrono::NaiveDate;
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::middleware::AuthUser;
use crate::models::{CreateWorkoutLog, CreateWorkoutSession, Exercise, WorkoutLogWithExercise, WorkoutSession};
use crate::repositories::{ExerciseRepository, WorkoutRepository};

#[derive(Clone)]
pub struct WorkoutsState {
    pub workout_repo: WorkoutRepository,
    pub exercise_repo: ExerciseRepository,
}

// Templates
#[derive(Template)]
#[template(path = "workouts/list.html")]
struct WorkoutsListTemplate {
    user: AuthUser,
    workouts: Vec<WorkoutSession>,
    page: i64,
    total_pages: i64,
}

#[derive(Template)]
#[template(path = "workouts/new.html")]
struct NewWorkoutTemplate {
    user: AuthUser,
    today: NaiveDate,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "workouts/show.html")]
struct ShowWorkoutTemplate {
    user: AuthUser,
    workout: WorkoutSession,
    logs: Vec<WorkoutLogWithExercise>,
    exercises: Vec<Exercise>,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "workouts/edit.html")]
struct EditWorkoutTemplate {
    user: AuthUser,
    workout: WorkoutSession,
    error: Option<String>,
}

// Query params
#[derive(Deserialize)]
pub struct ListQuery {
    page: Option<i64>,
}

// Handlers
pub async fn list(
    State(state): State<WorkoutsState>,
    auth_user: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Response> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = 10;
    let offset = (page - 1) * per_page;

    let workouts = state
        .workout_repo
        .find_sessions_by_user_paginated(&auth_user.id, per_page, offset)
        .await?;

    let total = state.workout_repo.count_sessions_by_user(&auth_user.id).await?;
    let total_pages = (total + per_page - 1) / per_page;

    let template = WorkoutsListTemplate {
        user: auth_user,
        workouts,
        page,
        total_pages,
    };

    Ok(Html(template.render().map_err(|e| AppError::Internal(e.to_string()))?).into_response())
}

pub async fn new_page(auth_user: AuthUser) -> Result<Response> {
    let today = chrono::Local::now().date_naive();

    let template = NewWorkoutTemplate {
        user: auth_user,
        today,
        error: None,
    };

    Ok(Html(template.render().map_err(|e| AppError::Internal(e.to_string()))?).into_response())
}

pub async fn create(
    State(state): State<WorkoutsState>,
    auth_user: AuthUser,
    Form(form): Form<CreateWorkoutSession>,
) -> Result<Response> {
    let workout = state
        .workout_repo
        .create_session(&auth_user.id, form.date, form.notes.as_deref())
        .await?;

    Ok(Redirect::to(&format!("/workouts/{}", workout.id)).into_response())
}

pub async fn show(
    State(state): State<WorkoutsState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    let workout = state
        .workout_repo
        .find_session_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Workout not found".to_string()))?;

    // Verify ownership
    if workout.user_id != auth_user.id {
        return Err(AppError::NotFound("Workout not found".to_string()));
    }

    let logs = state.workout_repo.find_logs_by_session(&id).await?;
    let exercises = state.exercise_repo.find_available_for_user(&auth_user.id).await?;

    let template = ShowWorkoutTemplate {
        user: auth_user,
        workout,
        logs,
        exercises,
        error: None,
    };

    Ok(Html(template.render().map_err(|e| AppError::Internal(e.to_string()))?).into_response())
}

pub async fn edit_page(
    State(state): State<WorkoutsState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    let workout = state
        .workout_repo
        .find_session_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Workout not found".to_string()))?;

    if workout.user_id != auth_user.id {
        return Err(AppError::NotFound("Workout not found".to_string()));
    }

    let template = EditWorkoutTemplate {
        user: auth_user,
        workout,
        error: None,
    };

    Ok(Html(template.render().map_err(|e| AppError::Internal(e.to_string()))?).into_response())
}

#[derive(Deserialize)]
pub struct UpdateWorkoutForm {
    pub date: NaiveDate,
    pub notes: Option<String>,
}

pub async fn update(
    State(state): State<WorkoutsState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
    Form(form): Form<UpdateWorkoutForm>,
) -> Result<Response> {
    state
        .workout_repo
        .update_session(&id, &auth_user.id, Some(form.date), form.notes.as_deref())
        .await?;

    Ok(Redirect::to(&format!("/workouts/{}", id)).into_response())
}

pub async fn delete(
    State(state): State<WorkoutsState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    state.workout_repo.delete_session(&id, &auth_user.id).await?;
    Ok(Redirect::to("/workouts").into_response())
}

// Workout Logs
pub async fn add_log(
    State(state): State<WorkoutsState>,
    auth_user: AuthUser,
    Path(session_id): Path<String>,
    Form(form): Form<CreateWorkoutLog>,
) -> Result<Response> {
    // Verify session ownership
    let session = state
        .workout_repo
        .find_session_by_id(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Workout not found".to_string()))?;

    if session.user_id != auth_user.id {
        return Err(AppError::NotFound("Workout not found".to_string()));
    }

    // Get next set number
    let set_number = state
        .workout_repo
        .get_next_set_number(&session_id, &form.exercise_id)
        .await?;

    // Create the log
    let log = state
        .workout_repo
        .create_log(
            &session_id,
            &form.exercise_id,
            set_number,
            form.reps,
            form.weight,
            form.rpe,
        )
        .await?;

    // Check for PR
    let current_pr = state
        .workout_repo
        .find_pr(&auth_user.id, &form.exercise_id, "max_weight")
        .await?;

    let is_new_pr = match current_pr {
        Some(pr) => form.weight > pr.value,
        None => true,
    };

    if is_new_pr {
        state
            .workout_repo
            .upsert_pr(&auth_user.id, &form.exercise_id, "max_weight", form.weight)
            .await?;
        state.workout_repo.mark_as_pr(&log.id).await?;
    }

    Ok(Redirect::to(&format!("/workouts/{}", session_id)).into_response())
}

pub async fn delete_log(
    State(state): State<WorkoutsState>,
    auth_user: AuthUser,
    Path((session_id, log_id)): Path<(String, String)>,
) -> Result<Response> {
    // Verify session ownership
    let session = state
        .workout_repo
        .find_session_by_id(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Workout not found".to_string()))?;

    if session.user_id != auth_user.id {
        return Err(AppError::NotFound("Workout not found".to_string()));
    }

    state.workout_repo.delete_log(&log_id, &session_id).await?;

    Ok(Redirect::to(&format!("/workouts/{}", session_id)).into_response())
}
