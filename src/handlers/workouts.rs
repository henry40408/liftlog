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
use crate::models::exercise::{ExerciseCategory, CATEGORIES};
use crate::models::{
    CreateWorkoutLog, CreateWorkoutSession, DynamicPR, Exercise, UpdateWorkoutLog, WorkoutLog,
    WorkoutLogWithExercise, WorkoutSession,
};
use crate::repositories::{ExerciseRepository, UserRepository, WorkoutRepository};

#[derive(Clone)]
pub struct WorkoutsState {
    pub workout_repo: WorkoutRepository,
    pub exercise_repo: ExerciseRepository,
    pub user_repo: UserRepository,
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
    categories: &'static [ExerciseCategory],
    exercise_prs: Vec<DynamicPR>,
    share_url: Option<String>,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "workouts/shared.html")]
struct SharedWorkoutTemplate {
    workout: WorkoutSession,
    logs: Vec<WorkoutLogWithExercise>,
    owner_username: String,
}

#[derive(Template)]
#[template(path = "workouts/edit.html")]
struct EditWorkoutTemplate {
    user: AuthUser,
    workout: WorkoutSession,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "workouts/edit_log.html")]
struct EditLogTemplate {
    user: AuthUser,
    workout: WorkoutSession,
    log: WorkoutLog,
    exercise_name: String,
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

    let total = state
        .workout_repo
        .count_sessions_by_user(&auth_user.id)
        .await?;
    let total_pages = (total + per_page - 1) / per_page;

    let template = WorkoutsListTemplate {
        user: auth_user,
        workouts,
        page,
        total_pages,
    };

    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn new_page(auth_user: AuthUser) -> Result<Response> {
    let today = chrono::Local::now().date_naive();

    let template = NewWorkoutTemplate {
        user: auth_user,
        today,
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

    let logs = state
        .workout_repo
        .find_logs_by_session_with_pr(&id, &auth_user.id)
        .await?;
    let exercises = state
        .exercise_repo
        .find_available_for_user(&auth_user.id)
        .await?;
    let exercise_prs = state
        .workout_repo
        .get_all_prs_by_user(&auth_user.id)
        .await?;

    let share_url = workout
        .share_token
        .as_ref()
        .map(|token| format!("/shared/{}", token));

    let template = ShowWorkoutTemplate {
        user: auth_user,
        workout,
        logs,
        exercises,
        categories: CATEGORIES,
        exercise_prs,
        share_url,
        error: None,
    };

    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
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

    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
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
    state
        .workout_repo
        .delete_session(&id, &auth_user.id)
        .await?;
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

    // Create the log (PR is computed dynamically)
    state
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

pub async fn edit_log_page(
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

    // Get the log
    let log = state
        .workout_repo
        .find_log_by_id(&log_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Log not found".to_string()))?;

    // Verify log belongs to this session
    if log.session_id != session_id {
        return Err(AppError::NotFound("Log not found".to_string()));
    }

    // Get exercise name
    let exercise = state
        .exercise_repo
        .find_by_id(&log.exercise_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Exercise not found".to_string()))?;

    let template = EditLogTemplate {
        user: auth_user,
        workout: session,
        log,
        exercise_name: exercise.name,
        error: None,
    };

    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn update_log(
    State(state): State<WorkoutsState>,
    auth_user: AuthUser,
    Path((session_id, log_id)): Path<(String, String)>,
    Form(form): Form<UpdateWorkoutLog>,
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

    state
        .workout_repo
        .update_log(&log_id, &session_id, form.reps, form.weight, form.rpe)
        .await?;

    Ok(Redirect::to(&format!("/workouts/{}", session_id)).into_response())
}

// Share functionality

pub async fn share_workout(
    State(state): State<WorkoutsState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    // Verify session ownership
    let session = state
        .workout_repo
        .find_session_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Workout not found".to_string()))?;

    if session.user_id != auth_user.id {
        return Err(AppError::NotFound("Workout not found".to_string()));
    }

    state
        .workout_repo
        .set_share_token(&id, &auth_user.id)
        .await?;

    Ok(Redirect::to(&format!("/workouts/{}", id)).into_response())
}

pub async fn revoke_share(
    State(state): State<WorkoutsState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Response> {
    // Verify session ownership
    let session = state
        .workout_repo
        .find_session_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Workout not found".to_string()))?;

    if session.user_id != auth_user.id {
        return Err(AppError::NotFound("Workout not found".to_string()));
    }

    state
        .workout_repo
        .revoke_share_token(&id, &auth_user.id)
        .await?;

    Ok(Redirect::to(&format!("/workouts/{}", id)).into_response())
}

pub async fn view_shared(
    State(state): State<WorkoutsState>,
    Path(token): Path<String>,
) -> Result<Response> {
    let workout = state
        .workout_repo
        .find_session_by_share_token(&token)
        .await?
        .ok_or_else(|| AppError::NotFound("Shared workout not found".to_string()))?;

    let logs = state
        .workout_repo
        .find_logs_by_session_for_share(&workout.id)
        .await?;

    let owner = state
        .user_repo
        .find_by_id(&workout.user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let template = SharedWorkoutTemplate {
        workout,
        logs,
        owner_username: owner.username,
    };

    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}
