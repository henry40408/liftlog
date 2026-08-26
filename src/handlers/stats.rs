use askama::Template;
use axum::{
    extract::{Path, Query, State},
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::middleware::AuthUser;
use crate::models::{
    ChartPoint, DynamicPR, Exercise, PersonalRecordSummary, WorkoutLogWithExercise,
    recent_pr_window_start,
};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "stats/index.html")]
struct StatsTemplate {
    user: AuthUser,
    workouts_this_week: i64,
    workouts_this_month: i64,
    total_volume: f64,
    total_workouts: i64,
    prs: Vec<PersonalRecordSummary>,
}

/// Geometry + flags used to draw the *default* server-rendered SVG.
/// Computed in the handler so the template stays declarative.
pub(crate) struct RenderedChart {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) padding_left: f64,
    pub(crate) padding_right: f64,
    pub(crate) padding_top: f64,
    /// Height of the plotting area, for the hover bands.
    pub(crate) plot_height: f64,
    pub(crate) points: Vec<RenderedPoint>,
    /// Polyline `points` attribute, e.g. "10,20 50,60 ..."
    pub(crate) polyline: String,
    /// Y-axis tick labels: (`y_pixel`, `label_text`)
    pub(crate) y_ticks: Vec<(f64, String)>,
    /// X-axis tick labels: (`x_pixel`, `label_text`). Only every Nth point is labeled.
    pub(crate) x_ticks: Vec<(f64, String)>,
}

pub(crate) struct RenderedPoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) is_pr: bool,
    /// Left edge and width of this point's hover band. The bands are
    /// server-rendered `<rect>`s carrying an SVG `<title>`, which browsers
    /// surface as a native tooltip with no scripting at all — the reason the
    /// figures are readable on hover without the JS tooltip.
    pub(crate) hit_x: f64,
    pub(crate) hit_width: f64,
    /// Tooltip text. Mirrors what `showTip` builds client-side.
    pub(crate) title: String,
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
    /// Rendered chart for the requested metric/range. `None` when fewer
    /// than 2 sessions.
    chart: Option<RenderedChart>,
    /// JSON-encoded full `Vec<ChartPoint>` for the client switcher.
    /// Already escaped: `</` → `<\/` so it cannot break out of `<script>`.
    chart_data_json: String,
    /// Active tab, as the query-string spellings the links use. The template
    /// compares these to mark `is-active`, so the server-rendered SVG and the
    /// highlighted tab cannot disagree.
    metric: &'static str,
    range: &'static str,
}

#[derive(Template)]
#[template(path = "stats/prs.html")]
struct PrsTemplate {
    user: AuthUser,
    prs: Vec<PersonalRecordSummary>,
}

const CHART_W: f64 = 600.0;
const CHART_H: f64 = 220.0;
const PAD_L: f64 = 44.0;
const PAD_R: f64 = 12.0;
const PAD_T: f64 = 14.0;
const PAD_B: f64 = 28.0;

/// Which series the chart plots.
///
/// The tabs are links, so this arrives in the query string and has to
/// survive anything typed there — hence `from_query` falling back to the
/// default rather than erroring. A bad `?metric=` is a stale bookmark, not
/// something worth a 400.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ChartMetric {
    #[default]
    TopSet,
    E1rm,
    Volume,
}

impl ChartMetric {
    fn from_query(raw: Option<&str>) -> Self {
        match raw {
            Some("e1rm") => Self::E1rm,
            Some("volume") => Self::Volume,
            _ => Self::TopSet,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::TopSet => "top_set",
            Self::E1rm => "e1rm",
            Self::Volume => "volume",
        }
    }

    /// Must agree with `metricValue` in `templates/stats/exercise.html`,
    /// which recomputes the same series client-side.
    fn value(self, p: &ChartPoint) -> f64 {
        match self {
            Self::TopSet => p.top_weight,
            Self::E1rm => p.e1rm,
            Self::Volume => p.volume,
        }
    }
}

/// How many sessions the chart covers.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ChartRange {
    #[default]
    Last20,
    All,
}

impl ChartRange {
    fn from_query(raw: Option<&str>) -> Self {
        match raw {
            Some("all") => Self::All,
            _ => Self::Last20,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Last20 => "20",
            Self::All => "all",
        }
    }
}

/// `?metric=&range=` on the exercise stats page. Both are optional and both
/// tolerate nonsense; see `ChartMetric::from_query`.
#[derive(Deserialize)]
pub struct ChartQuery {
    metric: Option<String>,
    range: Option<String>,
}

fn render_chart(
    points: &[ChartPoint],
    metric: ChartMetric,
    range: ChartRange,
) -> Option<RenderedChart> {
    if points.len() < 2 {
        return None;
    }

    let slice: Vec<&ChartPoint> = match range {
        ChartRange::All => points.iter().collect(),
        ChartRange::Last20 => points.iter().rev().take(20).rev().collect(),
    };
    if slice.len() < 2 {
        return None;
    }

    let values: Vec<f64> = slice.iter().map(|p| metric.value(p)).collect();
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
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

    // Hover bands, mirroring the geometry the client redraw uses: a full
    // band per interior point, half-bands at the two ends.
    let band_w = plot_w / (n as f64 - 1.0).max(1.0);

    for (i, p) in slice.iter().enumerate() {
        let x = PAD_L + (i as f64 / (n as f64 - 1.0)) * plot_w;
        // A "PR" dot is a running best *of the plotted series*, matching what
        // the client redraw does — on the volume tab the gold dots mark the
        // biggest sessions, not the heaviest top sets.
        let value = metric.value(p);
        let y = PAD_T + (1.0 - (value - y_min) / (y_max - y_min)) * plot_h;
        let is_pr = value > running_max;
        if is_pr {
            running_max = value;
        }

        let hit_x = if i == 0 { PAD_L } else { x - band_w / 2.0 };
        let hit_width = if i == 0 || i == n - 1 {
            band_w / 2.0
        } else {
            band_w
        };

        polyline_parts.push(format!("{x:.2},{y:.2}"));
        rendered_points.push(RenderedPoint {
            x,
            y,
            is_pr,
            hit_x,
            hit_width,
            title: format!(
                "{date}\nTop: {top_weight} kg × {top_reps}\ne1RM: {e1rm:.1} kg\nVolume: {volume:.0} kg",
                date = p.date,
                top_weight = p.top_weight,
                top_reps = p.top_reps,
                e1rm = p.e1rm,
                volume = p.volume
            ),
        });
    }

    let mut y_ticks = Vec::with_capacity(4);
    for i in 0..4 {
        let frac = i as f64 / 3.0;
        let y = PAD_T + frac * plot_h;
        let value = y_max - frac * (y_max - y_min);
        y_ticks.push((y, format!("{value:.0}")));
    }

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
        padding_top: PAD_T,
        plot_height: plot_h,
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
        .get_pr_summaries_by_user(&auth_user.id, recent_pr_window_start())
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
    Query(query): Query<ChartQuery>,
) -> Result<Response> {
    // The history/PR/metrics queries below are all scoped by `auth_user.id`, but
    // the exercise record itself is rendered, so fetching it unscoped disclosed
    // another user's exercise name and category.
    let exercise = state
        .exercise_repo
        .find_owned(&exercise_id, &auth_user.id)
        .await?;

    let history = state
        .workout_repo
        .get_exercise_history_with_pr(&auth_user.id, &exercise_id, 50, recent_pr_window_start())
        .await?;

    let pr = state
        .workout_repo
        .get_max_weight_for_exercise(&auth_user.id, &exercise_id)
        .await?;

    let metrics = state
        .workout_repo
        .get_session_metrics_for_exercise(&auth_user.id, &exercise_id)
        .await?;

    let metric = ChartMetric::from_query(query.metric.as_deref());
    let range = ChartRange::from_query(query.range.as_deref());

    let chart_points: Vec<ChartPoint> = metrics.iter().map(ChartPoint::from_metric).collect();
    let session_count = chart_points.len();
    let chart = render_chart(&chart_points, metric, range);
    let chart_data_json = encode_chart_data(&chart_points)?;

    let template = ExerciseStatsTemplate {
        user: auth_user,
        exercise,
        history,
        pr,
        session_count,
        chart,
        chart_data_json,
        metric: metric.as_str(),
        range: range.as_str(),
    };

    Ok(Html(template.render()?).into_response())
}

pub async fn prs_list(State(state): State<AppState>, auth_user: AuthUser) -> Result<Response> {
    let prs = state
        .workout_repo
        .get_pr_summaries_by_user(&auth_user.id, recent_pr_window_start())
        .await?;

    let template = PrsTemplate {
        user: auth_user,
        prs,
    };

    Ok(Html(template.render()?).into_response())
}
