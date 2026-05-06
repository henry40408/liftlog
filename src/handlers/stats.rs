use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Response},
};

use crate::error::{AppError, Result};
use crate::middleware::AuthUser;
use crate::models::{ChartPoint, DynamicPR, Exercise, WorkoutLogWithExercise};
use crate::state::AppState;

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

/// Geometry + flags used to draw the *default* server-rendered SVG.
/// Computed in the handler so the template stays declarative.
pub struct RenderedChart {
    pub width: f64,
    pub height: f64,
    pub padding_left: f64,
    pub padding_right: f64,
    pub points: Vec<RenderedPoint>,
    /// Polyline `points` attribute, e.g. "10,20 50,60 ..."
    pub polyline: String,
    /// Y-axis tick labels: (y_pixel, label_text)
    pub y_ticks: Vec<(f64, String)>,
    /// X-axis tick labels: (x_pixel, label_text). Only every Nth point is labeled.
    pub x_ticks: Vec<(f64, String)>,
}

pub struct RenderedPoint {
    pub x: f64,
    pub y: f64,
    pub is_pr: bool,
}

#[derive(Template)]
#[template(path = "stats/exercise.html")]
struct ExerciseStatsTemplate {
    user: AuthUser,
    exercise: Exercise,
    history: Vec<WorkoutLogWithExercise>,
    pr: Option<DynamicPR>,
    /// Total session count for this exercise (for the empty/sparse copy).
    session_count: usize,
    /// Default-state rendered chart. `None` when fewer than 2 sessions.
    chart: Option<RenderedChart>,
    /// JSON-encoded full `Vec<ChartPoint>` for the client switcher.
    /// Already escaped: `</` → `<\/` so it cannot break out of `<script>`.
    chart_data_json: String,
}

#[derive(Template)]
#[template(path = "stats/prs.html")]
struct PrsTemplate {
    user: AuthUser,
    prs: Vec<DynamicPR>,
}

const CHART_W: f64 = 600.0;
const CHART_H: f64 = 220.0;
const PAD_L: f64 = 44.0;
const PAD_R: f64 = 12.0;
const PAD_T: f64 = 14.0;
const PAD_B: f64 = 28.0;

fn render_default_chart(points: &[ChartPoint]) -> Option<RenderedChart> {
    if points.len() < 2 {
        return None;
    }

    // Default state: top set weight, last 20 sessions.
    let slice: Vec<&ChartPoint> = points.iter().rev().take(20).rev().collect();
    if slice.len() < 2 {
        return None;
    }

    let values: Vec<f64> = slice.iter().map(|p| p.top_weight).collect();
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    // Pad y range a bit so the line isn't flush against the top.
    let (y_min, y_max) = if (max - min).abs() < 1e-9 {
        (min - 1.0, max + 1.0)
    } else {
        let pad = (max - min) * 0.1;
        (min - pad, max + pad)
    };

    let plot_w = CHART_W - PAD_L - PAD_R;
    let plot_h = CHART_H - PAD_T - PAD_B;
    let n = slice.len();

    // Running max for PR detection.
    let mut running_max = f64::NEG_INFINITY;
    let mut rendered_points = Vec::with_capacity(n);
    let mut polyline_parts = Vec::with_capacity(n);

    for (i, p) in slice.iter().enumerate() {
        let x = PAD_L + (i as f64 / (n as f64 - 1.0)) * plot_w;
        let y = PAD_T + (1.0 - (p.top_weight - y_min) / (y_max - y_min)) * plot_h;
        let is_pr = p.top_weight > running_max;
        if is_pr {
            running_max = p.top_weight;
        }
        polyline_parts.push(format!("{:.2},{:.2}", x, y));
        rendered_points.push(RenderedPoint { x, y, is_pr });
    }

    // 4 evenly spaced y ticks.
    let mut y_ticks = Vec::with_capacity(4);
    for i in 0..4 {
        let frac = i as f64 / 3.0;
        let y = PAD_T + frac * plot_h;
        let value = y_max - frac * (y_max - y_min);
        y_ticks.push((y, format!("{:.0}", value)));
    }

    // Up to 5 x-axis date labels, evenly sampled.
    let label_count = n.min(5);
    let mut x_ticks = Vec::with_capacity(label_count);
    if label_count >= 2 {
        for i in 0..label_count {
            let idx = i * (n - 1) / (label_count - 1);
            let p = &rendered_points[idx];
            let label = slice[idx].date.format("%m-%d").to_string();
            x_ticks.push((p.x, label));
        }
    } else if let Some(p) = rendered_points.first() {
        x_ticks.push((p.x, slice[0].date.format("%m-%d").to_string()));
    }

    Some(RenderedChart {
        width: CHART_W,
        height: CHART_H,
        padding_left: PAD_L,
        padding_right: PAD_R,
        points: rendered_points,
        polyline: polyline_parts.join(" "),
        y_ticks,
        x_ticks,
    })
}

fn encode_chart_data(points: &[ChartPoint]) -> Result<String> {
    let json = serde_json::to_string(points).map_err(|e| AppError::Internal(e.to_string()))?;
    // Prevent `</script>` injection inside <script type="application/json">.
    Ok(json.replace("</", "<\\/"))
}

pub async fn index(State(state): State<AppState>, auth_user: AuthUser) -> Result<Response> {
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

    Ok(Html(template.render()?).into_response())
}

pub async fn exercise_stats(
    State(state): State<AppState>,
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

    let metrics = state
        .workout_repo
        .get_session_metrics_for_exercise(&auth_user.id, &exercise_id)
        .await?;

    let chart_points: Vec<ChartPoint> = metrics.iter().map(ChartPoint::from_metric).collect();
    let session_count = chart_points.len();
    let chart = render_default_chart(&chart_points);
    let chart_data_json = encode_chart_data(&chart_points)?;

    let template = ExerciseStatsTemplate {
        user: auth_user,
        exercise,
        history,
        pr,
        session_count,
        chart,
        chart_data_json,
    };

    Ok(Html(template.render()?).into_response())
}

pub async fn prs_list(State(state): State<AppState>, auth_user: AuthUser) -> Result<Response> {
    let prs = state
        .workout_repo
        .get_all_prs_by_user(&auth_user.id)
        .await?;

    let template = PrsTemplate {
        user: auth_user,
        prs,
    };

    Ok(Html(template.render()?).into_response())
}
