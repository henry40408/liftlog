use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Response},
};

use crate::error::{AppError, Result};
use crate::middleware::AuthUser;
use crate::models::WorkoutSession;
use crate::repositories::WorkoutRepository;

#[derive(Clone)]
pub struct DashboardState {
    pub workout_repo: WorkoutRepository,
}

#[derive(Template)]
#[template(path = "dashboard/index.html")]
struct DashboardTemplate {
    user: AuthUser,
    workouts_this_week: i64,
    workouts_this_month: i64,
    total_volume: f64,
    recent_workouts: Vec<WorkoutSession>,
}

pub async fn index(
    State(state): State<DashboardState>,
    auth_user: AuthUser,
) -> Result<Response> {
    let workouts_this_week = state.workout_repo.count_workouts_this_week(&auth_user.id).await?;
    let workouts_this_month = state.workout_repo.count_workouts_this_month(&auth_user.id).await?;
    let total_volume = state.workout_repo.get_total_volume_this_week(&auth_user.id).await?;
    let recent_workouts = state
        .workout_repo
        .find_sessions_by_user_paginated(&auth_user.id, 5, 0)
        .await?;

    let template = DashboardTemplate {
        user: auth_user,
        workouts_this_week,
        workouts_this_month,
        total_volume,
        recent_workouts,
    };

    Ok(Html(template.render().map_err(|e| AppError::Internal(e.to_string()))?).into_response())
}
