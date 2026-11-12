use axum::{
    extract::State,
    response::{IntoResponse, Response},
};

use crate::error::Result;
use crate::middleware::AuthUser;
use crate::state::AppState;
use crate::timezone;
use crate::views::dashboard::{DashboardView, dashboard};

pub async fn index(State(state): State<AppState>, auth_user: AuthUser) -> Result<Response> {
    let timezone = state.user_repo.find_timezone(&auth_user.id).await?;
    let today = timezone::today_in(timezone.as_deref());
    let week_start = timezone::window_start(today, 7);
    let month_start = timezone::window_start(today, 30);

    let workouts_this_week = state
        .workout_repo
        .count_workouts_this_week(&auth_user.id, week_start)
        .await?;
    let workouts_this_month = state
        .workout_repo
        .count_workouts_this_month(&auth_user.id, month_start)
        .await?;
    let total_volume = state
        .workout_repo
        .get_total_volume_this_week(&auth_user.id, week_start)
        .await?;
    let recent_workouts = state
        .workout_repo
        .find_sessions_by_user_paginated(&auth_user.id, 5, 0)
        .await?;

    let view = DashboardView {
        user: &auth_user,
        workouts_this_week,
        workouts_this_month,
        total_volume,
        recent_workouts: &recent_workouts,
        timezone_unset: timezone.is_none(),
    };

    Ok(dashboard(&view).into_response())
}
