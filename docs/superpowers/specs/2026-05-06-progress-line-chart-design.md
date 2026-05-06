# Progress Line Chart for Per-Exercise Stats

Date: 2026-05-06
Status: Approved — awaiting implementation plan
Branch: `feat/progress-line-chart`

## Problem

`/stats/exercise/{id}` currently shows a single PR card and a flat table of the last 50 logs (`templates/stats/exercise.html`, `src/handlers/stats.rs:73-102`). A user reading that page cannot see at a glance whether their numbers are trending up. The goal is to add a session-level line chart that surfaces progress over time.

## Decisions

| Decision | Value | Rationale |
|---|---|---|
| Surface | Per-exercise stats page only (`/stats/exercise/{id}`) | Per-exercise progress is the focused intent; dashboard rollups and cross-exercise comparison are out of scope. |
| Default metric | Top set weight (`MAX(weight)` for that session and exercise) | Most intuitive; matches Strong / Hevy / FitNotes default. |
| Switchable metrics | e1RM (Epley: `top_weight × (1 + top_reps / 30)`); Total volume (`Σ weight × reps`) | Coach-relevant alternatives. e1RM rewards same-weight-more-reps; volume reflects hypertrophy work. |
| Default range | Last 20 sessions | Avoids early-training light weights compressing the recent scale. |
| Switchable range | All-time (every session that includes the exercise) | For longer-arc reflection. |
| X axis | Equal-spaced session ordering, oldest → newest, with date labels under selected ticks | Industry standard; ignores rest-week irregularity that calendar-time scaling would amplify. |
| PR highlight | Gold dot when the point is the running max of the active metric within the displayed range | Reuses `--gold` token; aligns with existing `pr-badge` styling. |
| Empty / sparse state | 0 sessions → "No data yet" message; 1 session → single dot + "Need more sessions to see trend"; ≥ 2 → full chart | Line chart needs ≥ 2 points to be meaningful. |
| Rendering | Server-rendered inline SVG (default state) + JSON-embedded dataset for client-side re-rendering | Chart is visible without JS; JS adds switching and tooltips on top. |
| Interactivity | Vanilla inline JS in the template, no third-party library | Keeps the project's zero-JS-dependency posture. Same pattern as existing `cloneSet` / `fillLastWeight` inline scripts. |
| Top-set tie-break | When two sets share `MAX(weight)` in a session, take the one with the most reps | Yields the higher e1RM, which is the more flattering and physically more correct interpretation of "top set". |

## Data Model

No schema change. New repository method on `WorkoutRepository`:

```rust
pub struct ExerciseSessionMetric {
    pub session_id: String,
    pub date: NaiveDate,
    pub top_weight: f64,
    pub top_reps: i32,
    pub volume: f64,
}

pub async fn get_session_metrics_for_exercise(
    &self,
    user_id: &str,
    exercise_id: &str,
) -> Result<Vec<ExerciseSessionMetric>>
```

Returns one row per `workout_session` that contains at least one log for the exercise, ordered `ws.date ASC, ws.created_at ASC` (oldest first — natural plotting order).

SQL:

```sql
SELECT
    ws.id   AS session_id,
    ws.date AS date,
    MAX(wl.weight) AS top_weight,
    (SELECT wl2.reps
       FROM workout_logs wl2
      WHERE wl2.session_id  = ws.id
        AND wl2.exercise_id = wl.exercise_id
      ORDER BY wl2.weight DESC, wl2.reps DESC
      LIMIT 1) AS top_reps,
    SUM(wl.weight * wl.reps) AS volume
FROM workout_logs wl
JOIN workout_sessions ws ON wl.session_id = ws.id
WHERE ws.user_id = ? AND wl.exercise_id = ?
GROUP BY ws.id
ORDER BY ws.date ASC, ws.created_at ASC;
```

e1RM is **not** stored or returned by SQL — the handler / template / client compute it from `(top_weight, top_reps)` via Epley. This keeps the repository surface narrow and the formula in one place (Rust + JS).

## Handler Flow

`stats::exercise_stats` (`src/handlers/stats.rs:73`) gains a third repository call: `get_session_metrics_for_exercise`.

The handler then:

1. Computes a `Vec<ChartPoint>` from the metric rows. Each `ChartPoint` carries `{ date, top_set, e1rm, volume }` with `e1rm = top_weight * (1.0 + top_reps as f64 / 30.0)`.
2. Picks the default-rendered slice: `chart_points.iter().rev().take(20).rev()` if there are more than 20 sessions, else all of them.
3. Pre-computes SVG geometry (x positions, y positions, polyline `points` string, axis tick positions and labels) for the **default** state (top set, last 20). This becomes `Option<RenderedChart>` — `None` when fewer than 2 points.
4. Serializes the **full** `chart_points` list to JSON for the client. The client uses this for any subsequent metric / range switch without a server round trip.
5. Detects PR points for the default state (running max of `top_set` over the displayed slice) and tags those `(x, y)` coordinates as gold.

Template additions (`templates/stats/exercise.html`):

```
Personal Record   (existing)
Progress Trend    NEW — chart card here
Recent History    (existing)
```

The new section is a styled card containing:

- Metric tabs (Top Set / e1RM / Volume) — buttons with `data-metric` attributes.
- Range tabs (20 / All) — buttons with `data-range` attributes.
- The SVG chart (`viewBox="0 0 600 220"`, `width="100%"`).
- A hidden `<script type="application/json" id="exercise-chart-data">` containing the full dataset.
- A hidden tooltip `<div>` that the JS positions and fills on hover / tap.
- Helper text on no-JS / sparse-data states.
- Legend showing accent dot = session top set, gold dot = PR.

## Client-Side Script

Inline `<script>` block at the bottom of the page (matching the `cloneSet` / `fillLastWeight` placement in `templates/workouts/show.html`). Approximate budget: **80 lines of vanilla JS**, no imports.

Responsibilities:

- Read full dataset from the `<script type="application/json">` block.
- Track `activeMetric` (`top_set` | `e1rm` | `volume`) and `activeRange` (`20` | `all`).
- `redraw()`: compute slice, axes, polyline string, dot positions, PR detection, then update the SVG DOM (replace `<polyline>` `points`, axis labels, dot list).
- `showTooltip(point, x, y)` / `hideTooltip()`: position absolutely, show `date`, top set weight, reps, e1RM, volume — always all three metrics regardless of `activeMetric`.
- Pointer / touch handling: invisible `<rect>` hit-areas spanning each x-band; `pointerenter` / `pointerleave` for desktop, `pointerdown` for touch.

The initial server-rendered SVG remains correct and visible if JS fails. Metric / range tabs are JS-only in v1: when JS is disabled, the user sees the default chart and the tabs are inert.

## Empty / Sparse States

- **0 sessions for the exercise**: render a `<p class="muted">No progress data yet — log this exercise to see your trend.</p>` in place of the chart card body.
- **1 session**: render a minimal SVG with one centered dot plus `<p class="muted">Need at least 2 sessions to draw a trend.</p>`. Tabs remain visible but inert until ≥ 2 points exist for the active metric/range slice.

## Testing

### Repository unit tests (`src/repositories/workout_repo.rs`)

- `get_session_metrics_for_exercise_empty`: no sessions → empty Vec.
- `get_session_metrics_for_exercise_single_session`: one session, multiple sets — returns one row with correct `top_weight`, `top_reps` (tie-break verified), `volume`.
- `get_session_metrics_for_exercise_multiple_sessions_ordered_asc`: rows ordered oldest to newest.
- `get_session_metrics_for_exercise_excludes_other_exercises`: a session containing two exercises only contributes to the queried exercise's row.
- `get_session_metrics_for_exercise_user_isolation`: another user's data does not leak.
- `get_session_metrics_for_exercise_top_set_tie_picks_higher_reps`: two sets at the same max weight with different reps — `top_reps` is the higher one.

### Handler integration tests (`tests/stats_test.rs`)

- `exercise_stats_chart_renders_with_two_or_more_sessions`: SVG `<polyline>` element present; embedded `<script type="application/json" id="exercise-chart-data">` is valid JSON with the expected shape.
- `exercise_stats_chart_renders_sparse_state_with_one_session`: helper text "Need at least 2 sessions" present; no `<polyline>`.
- `exercise_stats_chart_renders_empty_state_with_no_logs`: helper text "No progress data yet" present.
- `exercise_stats_chart_pr_dots_match_expected_indices`: assert PR markers (`class="ll-dot-pr"` or equivalent) appear at the running-max indices for the default metric.

Visual / interactive testing is manual: load page in a browser, verify metric and range tabs swap the chart in place, tooltip appears on hover and tap, gold dots match PR sessions.

## Commit Plan

Single branch `feat/progress-line-chart`. Two commits, both with passing tests:

1. **`feat(stats): add session-level metrics query for exercise progress`**
   New `ExerciseSessionMetric` model, `WorkoutRepository::get_session_metrics_for_exercise`, repo unit tests.
2. **`feat(stats): render progress line chart on per-exercise page`**
   Handler changes, `templates/stats/exercise.html` chart section + inline script, `tests/stats_test.rs` updates, manual browser smoke check noted in PR description.

## Non-Goals

- Dashboard-level cross-exercise progress chart.
- Calendar-time X axis (would distort visual cadence around rest weeks).
- Range picker beyond 20 / All (e.g. 30d / 90d / 1y).
- Zoom / pan / brush selection.
- Animation on metric / range switch.
- Server-side fallback for the metric and range tabs — JS-only in v1. Adding `?metric=…&range=…` query params later is a small addition if needed.
- Image export of the chart.
- Comparison overlays (multiple exercises on one chart).
- Persistence of the user's last metric / range choice across visits.

## Open Questions

None at write time.
