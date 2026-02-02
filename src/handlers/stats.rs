use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Response},
};

use crate::error::{AppError, Result};
use crate::middleware::AuthUser;
use crate::models::{DynamicPR, Exercise, WorkoutLogWithExercise};
use crate::repositories::{ExerciseRepository, WorkoutRepository};

#[derive(Clone)]
pub struct StatsState {
    pub workout_repo: WorkoutRepository,
    pub exercise_repo: ExerciseRepository,
}

#[derive(Template)]
#[template(path = "stats/index.html")]
struct StatsTemplate {
    user: AuthUser,
    workouts_this_week: i64,
    workouts_this_month: i64,
    total_volume: f64,
    total_workouts: i64,
    prs: Vec<DynamicPR>,
}

#[derive(Template)]
#[template(path = "stats/exercise.html")]
struct ExerciseStatsTemplate {
    user: AuthUser,
    exercise: Exercise,
    history: Vec<WorkoutLogWithExercise>,
    pr: Option<DynamicPR>,
}

#[derive(Template)]
#[template(path = "stats/prs.html")]
struct PrsTemplate {
    user: AuthUser,
    prs: Vec<DynamicPR>,
}

pub async fn index(State(state): State<StatsState>, auth_user: AuthUser) -> Result<Response> {
    let workouts_this_week = state
        .workout_repo
        .count_workouts_this_week(&auth_user.id)
        .await?;
    let workouts_this_month = state
        .workout_repo
        .count_workouts_this_month(&auth_user.id)
        .await?;
    let total_volume = state
        .workout_repo
        .get_total_volume_this_week(&auth_user.id)
        .await?;
    let total_workouts = state
        .workout_repo
        .count_sessions_by_user(&auth_user.id)
        .await?;
    let prs = state
        .workout_repo
        .get_all_prs_by_user(&auth_user.id)
        .await?;

    let template = StatsTemplate {
        user: auth_user,
        workouts_this_week,
        workouts_this_month,
        total_volume,
        total_workouts,
        prs,
    };

    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn exercise_stats(
    State(state): State<StatsState>,
    auth_user: AuthUser,
    Path(exercise_id): Path<String>,
) -> Result<Response> {
    let exercise = state
        .exercise_repo
        .find_by_id(&exercise_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Exercise not found".to_string()))?;

    let history = state
        .workout_repo
        .get_exercise_history_with_pr(&auth_user.id, &exercise_id, 50)
        .await?;

    let pr = state
        .workout_repo
        .get_max_weight_for_exercise(&auth_user.id, &exercise_id)
        .await?;

    let template = ExerciseStatsTemplate {
        user: auth_user,
        exercise,
        history,
        pr,
    };

    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}

pub async fn prs_list(State(state): State<StatsState>, auth_user: AuthUser) -> Result<Response> {
    let prs = state
        .workout_repo
        .get_all_prs_by_user(&auth_user.id)
        .await?;

    let template = PrsTemplate {
        user: auth_user,
        prs,
    };

    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .into_response())
}
