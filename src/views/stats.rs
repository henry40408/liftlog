use maud::{Markup, PreEscaped, html};

use super::layout::{nav, page};
use crate::handlers::stats::RenderedChart;
use crate::middleware::AuthUser;
use crate::models::{DynamicPR, Exercise, PersonalRecordSummary, WorkoutLogWithExercise};

pub struct StatsView<'a> {
    pub user: &'a AuthUser,
    pub workouts_this_week: i64,
    pub workouts_this_month: i64,
    pub total_volume: f64,
    pub total_workouts: i64,
    pub prs: &'a [PersonalRecordSummary],
}

fn stat_card(value: impl std::fmt::Display, label: &str) -> Markup {
    html! {
        div class="stat-card" {
            div class="stat-value" { (value) }
            div class="stat-label" { (label) }
        }
    }
}

fn date_time(at: chrono::DateTime<chrono::Utc>) -> Markup {
    html! {
        time datetime=(at.to_rfc3339()) data-fmt="date" { (at.format("%Y-%m-%d")) }
    }
}

fn empty_dash() -> Markup {
    html! { span class="muted" { (PreEscaped("&mdash;")) } }
}

fn recent_pr_cell(pr: &PersonalRecordSummary) -> Markup {
    let color = if pr.recent_is_all_time() {
        "var(--gold)"
    } else {
        "var(--accent)"
    };
    html! {
        td data-label="PR (1M)" style={"color: " (color) "; font-weight: 600;"} {
            @if let Some(v) = pr.recent_value { (v) } @else { (empty_dash()) }
        }
    }
}

fn exercise_cell(pr: &PersonalRecordSummary) -> Markup {
    html! {
        td data-label="Exercise" { a href={"/stats/exercise/" (pr.exercise_id)} { (pr.exercise_name) } }
    }
}

fn no_prs() -> Markup {
    html! {
        div class="empty-state" { p class="muted" { "No personal records yet. Start lifting!" } }
    }
}

pub fn index(view: &StatsView) -> Markup {
    let body = html! {
        (nav(view.user))
        main {
            div class="page-header" { h1 { "Statistics" } }
            div class="stats-grid" {
                (stat_card(view.total_workouts, "Total Workouts"))
                (stat_card(view.workouts_this_week, "This Week"))
                (stat_card(view.workouts_this_month, "This Month"))
                (stat_card(view.total_volume, "Volume (Week)"))
            }
            h2 { "Recent Personal Records" }
            @if view.prs.is_empty() {
                (no_prs())
            } @else {
                table class="data-table" {
                    thead { tr { th { "Exercise" } th { "PR (All)" } th { "PR (1M)" } th { "Date" } } }
                    tbody {
                        @for pr in view.prs {
                            tr {
                                (exercise_cell(pr))
                                td data-label="PR (All)" style="color: var(--gold); font-weight: 600;" { (pr.all_time_value) }
                                (recent_pr_cell(pr))
                                td data-label="Date" { (date_time(pr.all_time_achieved_at)) }
                            }
                        }
                    }
                }
                p class="mt-4" { a href="/stats/prs" { "View All PRs " (PreEscaped("&rarr;")) } }
            }
        }
    };
    page("Stats - LiftLog", body)
}

pub fn prs(user: &AuthUser, prs: &[PersonalRecordSummary]) -> Markup {
    let body = html! {
        (nav(user))
        main {
            div class="page-header" { h1 { "Personal Records" } }
            @if prs.is_empty() {
                (no_prs())
            } @else {
                table class="data-table" {
                    thead { tr { th { "Exercise" } th { "PR (All)" } th { "Date" } th { "PR (1M)" } th { "Date" } } }
                    tbody {
                        @for pr in prs {
                            tr {
                                (exercise_cell(pr))
                                td data-label="PR (All)" style="color: var(--gold); font-weight: 600;" { (pr.all_time_value) }
                                td data-label="Date" { (date_time(pr.all_time_achieved_at)) }
                                (recent_pr_cell(pr))
                                td data-label="Date" {
                                    @if let Some(at) = pr.recent_achieved_at { (date_time(at)) } @else { (empty_dash()) }
                                }
                            }
                        }
                    }
                }
            }
            a href="/stats" class="back-link" { (PreEscaped("&larr;")) " Back to Stats" }
        }
    };
    page("Personal Records - LiftLog", body)
}

pub struct ExerciseStatsView<'a> {
    pub user: &'a AuthUser,
    pub exercise: &'a Exercise,
    pub history: &'a [WorkoutLogWithExercise],
    pub pr: Option<&'a DynamicPR>,
    /// Total session count for this exercise (for the empty/sparse copy).
    pub session_count: usize,
    /// Rendered chart for the requested metric/range. `None` when fewer
    /// than 2 sessions.
    pub chart: Option<&'a RenderedChart>,
    /// JSON-encoded full `Vec<ChartPoint>` for the client switcher.
    /// Already escaped: `</` → `<\/` so it cannot break out of `<script>`.
    pub chart_data_json: &'a str,
    /// Active tab, in query-string spelling, so the tab and SVG agree.
    pub metric: &'static str,
    pub range: &'static str,
}

/// Client-side redraw of the chart; `{{METRIC}}` and `{{RANGE}}` are the
/// server's selection. Must stay in sync with `render_chart`.
const CHART_JS: &str = include_str!("stats_exercise.js");

pub fn exercise(view: &ExerciseStatsView) -> Markup {
    let body = html! {
        (nav(view.user))
        main {
            div class="page-header" {
                h1 { (view.exercise.name) }
                div class="subtitle" { (view.exercise.category) }
            }
            h2 { "Personal Record" }
            @if let Some(p) = view.pr {
                div class="card card-gold" style="margin-bottom: var(--sp-6); display: inline-block;" {
                    div style="display: flex; align-items: baseline; gap: var(--sp-4);" {
                        span class="stat-value" style="font-size: var(--font-4xl);" { (p.value) }
                        " "
                        span class="text-secondary text-sm" {
                            "kg " (PreEscaped("&middot;")) " " (date_time(p.achieved_at))
                        }
                    }
                }
            } @else {
                p class="muted" { "No PR for this exercise yet." }
            }
            h2 { "Progress Trend" }
            div class="card" style="margin-bottom: var(--sp-6);" { (progress_trend(view)) }
            h2 { "Recent History" }
            @if view.history.is_empty() {
                p class="muted" { "No history for this exercise." }
            } @else {
                (history_table(view.history))
            }
            a href="/exercises" class="back-link" { (PreEscaped("&larr;")) " Back to Exercises" }
        }
        @if view.session_count >= 2 {
            script {
                (PreEscaped(CHART_JS.replace("{{METRIC}}", view.metric).replace("{{RANGE}}", view.range)))
            }
        }
    };
    page(&format!("{} Stats - LiftLog", view.exercise.name), body)
}

fn history_table(history: &[WorkoutLogWithExercise]) -> Markup {
    html! {
        table class="data-table" {
            thead { tr { th { "Set" } th { "Weight" } th { "Reps" } th { "RPE" } th {} } }
            tbody {
                @for log in history {
                    tr {
                        td { (log.set_number) }
                        td style="color: var(--text-primary); font-weight: 500;" { (log.weight) }
                        td { (log.reps) }
                        td { @if let Some(r) = log.rpe { (r) } @else { "-" } }
                        td {
                            @if log.is_pr {
                                span class="pr-badge" title="All-time best for this exercise" { "PR" }
                            } @else if log.is_recent_pr {
                                span class="pr-badge pr-badge-recent" title="Best for this exercise in the last month" { "PR 1M" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn tab_class(active: bool) -> &'static str {
    if active {
        "btn btn-sm btn-tab is-active"
    } else {
        "btn btn-sm btn-tab"
    }
}

fn metric_tab(view: &ExerciseStatsView, metric: &str, label: &str) -> Markup {
    html! {
        a href={"?metric=" (metric) "&range=" (view.range)} class=(tab_class(view.metric == metric)) data-metric=(metric) { (label) }
    }
}

fn range_tab(view: &ExerciseStatsView, range: &str, label: &str) -> Markup {
    html! {
        a href={"?metric=" (view.metric) "&range=" (range)} class=(tab_class(view.range == range)) data-range=(range) { (label) }
    }
}

fn progress_trend(view: &ExerciseStatsView) -> Markup {
    if view.session_count == 0 {
        return html! { p class="muted" { "No progress data yet — log this exercise to see your trend." } };
    }
    html! {
        // Links so every series is reachable without scripts; the script
        // below redraws in place instead.
        div id="exercise-chart-controls" style="display: flex; gap: var(--sp-4); flex-wrap: wrap; margin-bottom: var(--sp-4);" {
            div role="group" aria-label="Metric" style="display: flex; gap: var(--sp-2);" {
                (metric_tab(view, "top_set", "Top Set"))
                " "
                (metric_tab(view, "e1rm", "e1RM"))
                " "
                (metric_tab(view, "volume", "Volume"))
            }
            div role="group" aria-label="Range" style="display: flex; gap: var(--sp-2);" {
                (range_tab(view, "20", "Last 20"))
                " "
                (range_tab(view, "all", "All"))
            }
        }
        div id="exercise-chart-wrap" style="position: relative;" {
            (chart_svg(view))
            div id="exercise-chart-tooltip"
                style="position: absolute; display: none; pointer-events: none; background: var(--bg-elevated); border: 1px solid var(--border); padding: 8px 10px; font-size: 12px; border-radius: 6px; color: var(--text-primary); white-space: nowrap; z-index: 2;" {}
        }
        @if view.session_count == 1 {
            p class="muted" style="margin-top: var(--sp-4);" { "Need at least 2 sessions to draw a trend." }
        }
        div style="display: flex; gap: var(--sp-4); margin-top: var(--sp-3); font-size: 12px; color: var(--text-secondary);" {
            span { span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:var(--accent);vertical-align:middle;margin-right:4px;" {} "Session" }
            " "
            span { span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:var(--gold);vertical-align:middle;margin-right:4px;" {} "Running PR" }
        }
        // `chart_data_json` has `</` escaped to `<\/` (see `encode_chart_data`), so
        // user-entered text cannot close this script element.
        script type="application/json" id="exercise-chart-data" { (PreEscaped(view.chart_data_json)) }
    }
}

fn chart_svg(view: &ExerciseStatsView) -> Markup {
    html! {
        svg id="exercise-chart" viewBox="0 0 600 220" width="100%" preserveAspectRatio="xMidYMid meet" role="img" aria-label="Progress line chart" {
            @if let Some(c) = view.chart {
                (chart_body(c))
            } @else if view.session_count == 1 {
                circle cx="300" cy="110" r="5" fill="var(--accent)" stroke="var(--bg-root)" stroke-width="1.5" {}
            }
            // No-JS tooltips via SVG <title>; the client redraw replaces them.
            g id="chart-hit-areas" {
                @if let Some(c) = view.chart {
                    @for pt in &c.points {
                        rect x=(pt.hit_x) y=(c.padding_top) width=(pt.hit_width) height=(c.plot_height)
                            fill="transparent" style="cursor:crosshair" { title { (pt.title) } }
                    }
                }
            }
        }
    }
}

fn chart_body(c: &RenderedChart) -> Markup {
    html! {
        g id="chart-y-grid" {
            @for tick in &c.y_ticks {
                line x1=(c.padding_left) x2=(c.width - c.padding_right) y1=(tick.0) y2=(tick.0)
                    stroke="var(--border-light)" stroke-width="1" {}
                text x=(c.padding_left - 6.0) y=(tick.0 + 4.0)
                    text-anchor="end" font-size="11" fill="var(--text-secondary)" { (tick.1) }
            }
        }
        g id="chart-x-labels" {
            @for tick in &c.x_ticks {
                text x=(tick.0) y=(c.height - 8.0)
                    text-anchor="middle" font-size="11" fill="var(--text-secondary)" { (tick.1) }
            }
        }
        polyline id="chart-line" fill="none" stroke="var(--accent)" stroke-width="2" points=(c.polyline) {}
        g id="chart-dots" {
            @for pt in &c.points {
                circle cx=(pt.x) cy=(pt.y) r="4"
                    class=(if pt.is_pr { "ll-dot-pr" } else { "ll-dot" })
                    fill=(if pt.is_pr { "var(--gold)" } else { "var(--accent)" })
                    stroke="var(--bg-root)" stroke-width="1.5" {}
            }
        }
    }
}
