# Progress Line Chart Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a per-exercise session-level progress line chart on `/stats/exercise/{id}` showing top set weight (default), e1RM, or volume across last-20 or all-time sessions, with PR (running-max) gold dots.

**Architecture:** New repository method `WorkoutRepository::get_session_metrics_for_exercise` returning per-session aggregates (`top_weight`, `top_reps`, `volume`). Handler computes a `Vec<ChartPoint>` (with derived `e1rm`), renders the **default** state (top set, last 20) as an inline SVG via Askama, and embeds the **full** dataset as JSON in `<script type="application/json">`. Vanilla inline JS (~110 lines, IIFE) reads the JSON, swaps metric/range, redraws polyline + dots, and shows a tooltip. No third-party chart library.

**Tech Stack:** Rust 2021 / axum 0.8 / askama 0.15 / rusqlite 0.32 / serde_json (new dep) / vanilla JS / inline SVG.

**Spec:** `docs/superpowers/specs/2026-05-06-progress-line-chart-design.md`

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `Cargo.toml` | Modify | Add `serde_json = "1"` dependency. |
| `src/models/exercise_session_metric.rs` | Create | Define `ExerciseSessionMetric` (raw row) and `ChartPoint` (handler-shaped, with derived `e1rm`). `ChartPoint` derives `Serialize`. |
| `src/models/mod.rs` | Modify | Export `ExerciseSessionMetric` and `ChartPoint`. |
| `src/repositories/workout_repo.rs` | Modify | Add `get_session_metrics_for_exercise` method + 6 unit tests. |
| `src/handlers/stats.rs` | Modify | Compute `Vec<ChartPoint>`, build `RenderedChart` (geometry + PR flags) for default state, JSON-encode full dataset, pass to template. |
| `templates/stats/exercise.html` | Modify | Insert "Progress Trend" card between PR card and Recent History: tabs, SVG, JSON script, tooltip, helper text, inline JS IIFE. |
| `tests/stats_test.rs` | Modify | Add 4 integration tests (empty / sparse / full render / PR dots). |

Phase A produces commit 1 (model + repo + tests). Phase B produces commit 2 (handler + template + integration tests).

---

## Phase A — Repository layer

### Task 1: Add `serde_json` dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add the dependency**

In `[dependencies]`, add the new line in alphabetical-ish order (after `serde` is fine):

```toml
serde_json = "1"
```

The full `[dependencies]` section after edit must include the existing entries plus this one:

```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Verify it resolves**

Run: `cargo check`
Expected: compiles cleanly, `Cargo.lock` updates with `serde_json` entries. No warnings introduced.

- [ ] **Step 3: Stage but do not commit yet**

We commit at the end of Phase A together with the model + repo work.

---

### Task 2: Create `ExerciseSessionMetric` and `ChartPoint` models

**Files:**
- Create: `src/models/exercise_session_metric.rs`
- Modify: `src/models/mod.rs`

- [ ] **Step 1: Write the failing model tests**

Append to the **end** of the new file `src/models/exercise_session_metric.rs`:

```rust
use chrono::NaiveDate;
use rusqlite::Row;
use serde::Serialize;

use super::FromSqliteRow;

/// One row per workout session that contains the queried exercise.
/// Returned by `WorkoutRepository::get_session_metrics_for_exercise`.
#[derive(Debug, Clone)]
pub struct ExerciseSessionMetric {
    pub session_id: String,
    pub date: NaiveDate,
    pub top_weight: f64,
    pub top_reps: i32,
    pub volume: f64,
}

impl FromSqliteRow for ExerciseSessionMetric {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            session_id: row.get("session_id")?,
            date: row.get("date")?,
            top_weight: row.get("top_weight")?,
            top_reps: row.get("top_reps")?,
            volume: row.get("volume")?,
        })
    }
}

/// Chart-ready point. `e1rm` is derived via Epley from `(top_weight, top_reps)`.
/// Serialized into the page as JSON for the client-side switch handler.
#[derive(Debug, Clone, Serialize)]
pub struct ChartPoint {
    pub session_id: String,
    pub date: NaiveDate,
    pub top_weight: f64,
    pub top_reps: i32,
    pub volume: f64,
    pub e1rm: f64,
}

impl ChartPoint {
    pub fn from_metric(m: &ExerciseSessionMetric) -> Self {
        Self {
            session_id: m.session_id.clone(),
            date: m.date,
            top_weight: m.top_weight,
            top_reps: m.top_reps,
            volume: m.volume,
            e1rm: m.top_weight * (1.0 + m.top_reps as f64 / 30.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_point_from_metric_computes_epley_e1rm() {
        let metric = ExerciseSessionMetric {
            session_id: "s1".into(),
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            top_weight: 100.0,
            top_reps: 6,
            volume: 600.0,
        };
        let point = ChartPoint::from_metric(&metric);
        // Epley: 100 * (1 + 6/30) = 100 * 1.2 = 120.0
        assert!((point.e1rm - 120.0).abs() < 1e-9);
        assert_eq!(point.session_id, "s1");
        assert_eq!(point.top_weight, 100.0);
        assert_eq!(point.top_reps, 6);
        assert_eq!(point.volume, 600.0);
    }

    #[test]
    fn chart_point_e1rm_for_single_rep_equals_top_weight() {
        let metric = ExerciseSessionMetric {
            session_id: "s2".into(),
            date: NaiveDate::from_ymd_opt(2024, 1, 16).unwrap(),
            top_weight: 140.0,
            top_reps: 0,
            volume: 0.0,
        };
        let point = ChartPoint::from_metric(&metric);
        // 1 + 0/30 = 1.0 → e1rm equals top_weight
        assert!((point.e1rm - 140.0).abs() < 1e-9);
    }
}
```

- [ ] **Step 2: Wire up the module**

Edit `src/models/mod.rs`. Add `pub mod exercise_session_metric;` next to other `pub mod` lines and `pub use exercise_session_metric::{ChartPoint, ExerciseSessionMetric};` next to other `pub use` lines.

After the edit `src/models/mod.rs` should look like this:

```rust
pub mod exercise;
pub mod exercise_session_metric;
pub mod from_row;
pub mod personal_record;
pub mod user;
pub mod workout_log;
pub mod workout_session;

pub use exercise::{CreateExercise, Exercise, UpdateExercise};
pub use exercise_session_metric::{ChartPoint, ExerciseSessionMetric};
pub use from_row::FromSqliteRow;
pub use personal_record::{DynamicPR, LastExerciseWeight};
pub use user::{CreateUser, LoginCredentials, User, UserRole};
pub use workout_log::{CreateWorkoutLog, UpdateWorkoutLog, WorkoutLog, WorkoutLogWithExercise};
pub use workout_session::{CreateWorkoutSession, WorkoutSession};
```

- [ ] **Step 3: Run model tests to verify they pass**

Run: `cargo nextest run -p liftlog --lib models::exercise_session_metric`
Expected: `2 tests passed`. (If nextest matcher misses, fall back to `cargo nextest run --lib chart_point`.)

- [ ] **Step 4: Format and check**

Run: `cargo fmt`
Run: `cargo check`
Expected: no warnings about unused imports, no errors.

---

### Task 3: Repository test — empty result

**Files:**
- Modify: `src/repositories/workout_repo.rs`

- [ ] **Step 1: Write the failing test**

Add at the end of the existing `#[cfg(test)] mod tests { ... }` block, just before the final closing `}`:

```rust
#[tokio::test]
async fn test_get_session_metrics_for_exercise_empty() {
    let pool = setup_test_db();
    create_test_user(&pool, "user1");
    create_test_exercise(&pool, "ex-bench-press", "user1");
    let repo = WorkoutRepository::new(pool);

    let metrics = repo
        .get_session_metrics_for_exercise("user1", "ex-bench-press")
        .await
        .unwrap();

    assert!(metrics.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run --lib test_get_session_metrics_for_exercise_empty`
Expected: FAIL with `no method named 'get_session_metrics_for_exercise'` (compile error).

- [ ] **Step 3: Add the repository method (minimal first cut)**

In `src/repositories/workout_repo.rs`, just after `get_max_weight_for_exercise` (around the end of the "Dynamic Personal Records" block, before the `// Statistics` comment), add:

```rust
/// Per-session aggregates for a single exercise: top set weight, top set
/// reps (tie-broken by higher reps when weight ties), and total volume.
/// Ordered oldest → newest (`ws.date ASC, ws.created_at ASC`).
pub async fn get_session_metrics_for_exercise(
    &self,
    user_id: &str,
    exercise_id: &str,
) -> Result<Vec<crate::models::ExerciseSessionMetric>> {
    use crate::models::ExerciseSessionMetric;

    let pool = self.pool.clone();
    let user_id = user_id.to_string();
    let exercise_id = exercise_id.to_string();
    tokio::task::spawn_blocking(move || {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT
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
             ORDER BY ws.date ASC, ws.created_at ASC",
        )?;
        let rows = stmt
            .query_map(
                rusqlite::params![user_id, exercise_id],
                ExerciseSessionMetric::from_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await?
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run --lib test_get_session_metrics_for_exercise_empty`
Expected: PASS.

---

### Task 4: Repository test — single session, multi-set with tie-break

**Files:**
- Modify: `src/repositories/workout_repo.rs`

- [ ] **Step 1: Write the failing test**

Add inside the same `mod tests` block:

```rust
#[tokio::test]
async fn test_get_session_metrics_for_exercise_single_session_aggregates_correctly() {
    let pool = setup_test_db();
    create_test_user(&pool, "user1");
    create_test_exercise(&pool, "ex-bench-press", "user1");
    let repo = WorkoutRepository::new(pool);

    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let session = repo.create_session("user1", date, None).await.unwrap();

    // Three sets: 100x10, 110x8, 105x5 — top weight 110 with 8 reps; volume = 1000+880+525 = 2405
    repo.create_log(&session.id, "ex-bench-press", 1, 10, 100.0, None)
        .await
        .unwrap();
    repo.create_log(&session.id, "ex-bench-press", 2, 8, 110.0, None)
        .await
        .unwrap();
    repo.create_log(&session.id, "ex-bench-press", 3, 5, 105.0, None)
        .await
        .unwrap();

    let metrics = repo
        .get_session_metrics_for_exercise("user1", "ex-bench-press")
        .await
        .unwrap();

    assert_eq!(metrics.len(), 1);
    let m = &metrics[0];
    assert_eq!(m.session_id, session.id);
    assert_eq!(m.date, date);
    assert!((m.top_weight - 110.0).abs() < 1e-9);
    assert_eq!(m.top_reps, 8);
    assert!((m.volume - 2405.0).abs() < 1e-9);
}
```

- [ ] **Step 2: Run it to verify it passes**

Run: `cargo nextest run --lib test_get_session_metrics_for_exercise_single_session_aggregates_correctly`
Expected: PASS (the implementation from Task 3 already covers this).

---

### Task 5: Repository test — multiple sessions ordered ASC

**Files:**
- Modify: `src/repositories/workout_repo.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_get_session_metrics_for_exercise_multiple_sessions_ordered_asc() {
    let pool = setup_test_db();
    create_test_user(&pool, "user1");
    create_test_exercise(&pool, "ex-bench-press", "user1");
    let repo = WorkoutRepository::new(pool);

    // Insert sessions out of order: middle first, oldest second, newest third.
    let d_mid = NaiveDate::from_ymd_opt(2024, 1, 12).unwrap();
    let d_old = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
    let d_new = NaiveDate::from_ymd_opt(2024, 1, 20).unwrap();

    let s_mid = repo.create_session("user1", d_mid, None).await.unwrap();
    let s_old = repo.create_session("user1", d_old, None).await.unwrap();
    let s_new = repo.create_session("user1", d_new, None).await.unwrap();

    repo.create_log(&s_mid.id, "ex-bench-press", 1, 5, 100.0, None)
        .await
        .unwrap();
    repo.create_log(&s_old.id, "ex-bench-press", 1, 5, 90.0, None)
        .await
        .unwrap();
    repo.create_log(&s_new.id, "ex-bench-press", 1, 5, 110.0, None)
        .await
        .unwrap();

    let metrics = repo
        .get_session_metrics_for_exercise("user1", "ex-bench-press")
        .await
        .unwrap();

    assert_eq!(metrics.len(), 3);
    assert_eq!(metrics[0].date, d_old);
    assert_eq!(metrics[1].date, d_mid);
    assert_eq!(metrics[2].date, d_new);
}
```

- [ ] **Step 2: Run it to verify it passes**

Run: `cargo nextest run --lib test_get_session_metrics_for_exercise_multiple_sessions_ordered_asc`
Expected: PASS.

---

### Task 6: Repository test — excludes other exercises in same session

**Files:**
- Modify: `src/repositories/workout_repo.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_get_session_metrics_for_exercise_excludes_other_exercises() {
    let pool = setup_test_db();
    create_test_user(&pool, "user1");
    create_test_exercise(&pool, "ex-bench-press", "user1");
    create_test_exercise(&pool, "ex-squat", "user1");
    let repo = WorkoutRepository::new(pool);

    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let session = repo.create_session("user1", date, None).await.unwrap();

    // Bench: 100x10 (volume 1000), Squat: 200x5 (volume 1000) — same session.
    repo.create_log(&session.id, "ex-bench-press", 1, 10, 100.0, None)
        .await
        .unwrap();
    repo.create_log(&session.id, "ex-squat", 1, 5, 200.0, None)
        .await
        .unwrap();

    let metrics = repo
        .get_session_metrics_for_exercise("user1", "ex-bench-press")
        .await
        .unwrap();

    assert_eq!(metrics.len(), 1);
    let m = &metrics[0];
    assert!((m.top_weight - 100.0).abs() < 1e-9);
    assert_eq!(m.top_reps, 10);
    assert!((m.volume - 1000.0).abs() < 1e-9);
}
```

- [ ] **Step 2: Run it to verify it passes**

Run: `cargo nextest run --lib test_get_session_metrics_for_exercise_excludes_other_exercises`
Expected: PASS.

---

### Task 7: Repository test — user isolation

**Files:**
- Modify: `src/repositories/workout_repo.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_get_session_metrics_for_exercise_user_isolation() {
    let pool = setup_test_db();
    create_test_user(&pool, "user1");
    create_test_user(&pool, "user2");
    create_test_exercise(&pool, "ex-bench-press", "user1");
    let repo = WorkoutRepository::new(pool);

    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let s1 = repo.create_session("user1", date, None).await.unwrap();
    let s2 = repo.create_session("user2", date, None).await.unwrap();

    repo.create_log(&s1.id, "ex-bench-press", 1, 5, 100.0, None)
        .await
        .unwrap();
    repo.create_log(&s2.id, "ex-bench-press", 1, 5, 200.0, None)
        .await
        .unwrap();

    let metrics = repo
        .get_session_metrics_for_exercise("user1", "ex-bench-press")
        .await
        .unwrap();

    assert_eq!(metrics.len(), 1);
    assert!((metrics[0].top_weight - 100.0).abs() < 1e-9);
}
```

- [ ] **Step 2: Run it to verify it passes**

Run: `cargo nextest run --lib test_get_session_metrics_for_exercise_user_isolation`
Expected: PASS.

---

### Task 8: Repository test — top set tie-break picks higher reps

**Files:**
- Modify: `src/repositories/workout_repo.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_get_session_metrics_for_exercise_top_set_tie_picks_higher_reps() {
    let pool = setup_test_db();
    create_test_user(&pool, "user1");
    create_test_exercise(&pool, "ex-bench-press", "user1");
    let repo = WorkoutRepository::new(pool);

    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let session = repo.create_session("user1", date, None).await.unwrap();

    // Same max weight 100, two different rep counts. Tie-break must select 8 reps.
    repo.create_log(&session.id, "ex-bench-press", 1, 5, 100.0, None)
        .await
        .unwrap();
    repo.create_log(&session.id, "ex-bench-press", 2, 8, 100.0, None)
        .await
        .unwrap();
    repo.create_log(&session.id, "ex-bench-press", 3, 6, 100.0, None)
        .await
        .unwrap();

    let metrics = repo
        .get_session_metrics_for_exercise("user1", "ex-bench-press")
        .await
        .unwrap();

    assert_eq!(metrics.len(), 1);
    assert!((metrics[0].top_weight - 100.0).abs() < 1e-9);
    assert_eq!(metrics[0].top_reps, 8);
}
```

- [ ] **Step 2: Run it to verify it passes**

Run: `cargo nextest run --lib test_get_session_metrics_for_exercise_top_set_tie_picks_higher_reps`
Expected: PASS.

---

### Task 9: Phase A — format, full lib test run, commit

**Files:**
- All Phase A files.

- [ ] **Step 1: Format**

Run: `cargo fmt`

- [ ] **Step 2: Verify formatting and clean tree**

Run: `cargo fmt --all -- --check`
Expected: exits 0 with no output.
Run: `git status`
Expected: only the intended files are modified — `Cargo.toml`, `Cargo.lock`, `src/models/mod.rs`, `src/models/exercise_session_metric.rs` (new), `src/repositories/workout_repo.rs`. No stray `.codex` etc. staged.

- [ ] **Step 3: Run the whole lib test suite**

Run: `cargo nextest run --lib`
Expected: all tests pass; new tests visible: `chart_point_*` (2) + `test_get_session_metrics_for_exercise_*` (6) = 8 new passes.

- [ ] **Step 4: Commit Phase A**

```bash
git add Cargo.toml Cargo.lock src/models/mod.rs src/models/exercise_session_metric.rs src/repositories/workout_repo.rs
git commit -S -m "feat(stats): add session-level metrics query for exercise progress"
```

(GPG-sign per global rule. Do **not** pass `--no-gpg-sign`.)

Run `git log -1 --oneline` to verify the commit landed.

---

## Phase B — Handler, template, integration tests

### Task 10: Extend handler with chart computation

**Files:**
- Modify: `src/handlers/stats.rs`

- [ ] **Step 1: Replace the `ExerciseStatsTemplate` struct and `exercise_stats` handler**

Replace the existing `ExerciseStatsTemplate` struct and the `exercise_stats` function in `src/handlers/stats.rs` with the version below. Keep `index`, `prs_list`, `StatsTemplate`, `PrsTemplate` and the imports unchanged except for adding the new uses.

Update the `use` block at the top of the file. After this edit the imports section reads:

```rust
use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Response},
};

use crate::error::{AppError, Result};
use crate::middleware::AuthUser;
use crate::models::{ChartPoint, DynamicPR, Exercise, WorkoutLogWithExercise};
use crate::state::AppState;
```

Below `ExerciseStatsTemplate`, replace the struct and function definitions with:

```rust
/// Geometry + flags used to draw the *default* server-rendered SVG.
/// Computed in the handler so the template stays declarative.
pub struct RenderedChart {
    pub width: f64,
    pub height: f64,
    pub padding_left: f64,
    pub padding_right: f64,
    pub padding_top: f64,
    pub padding_bottom: f64,
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
        padding_top: PAD_T,
        padding_bottom: PAD_B,
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
```

- [ ] **Step 2: Confirm `AppError::Internal` exists**

Run: `grep -n "Internal" src/error.rs`
Expected: a variant like `Internal(String)` is present. If the variant has a different name (e.g. `Anyhow`, `Other`, `Internal { message }`), substitute it in `encode_chart_data` and re-run. If no such variant exists, instead use `?` with an `anyhow::Result` adapter:

```rust
let json = serde_json::to_string(points).map_err(|e| AppError::Internal(e.to_string()))?;
```
becomes
```rust
let json = serde_json::to_string(points)
    .map_err(|e| AppError::from(anyhow::anyhow!("encode chart data: {e}")))?;
```
Pick whichever variant the existing codebase uses for "non-domain serialization failure" in 1–2 other handlers.

- [ ] **Step 3: cargo check — handler compiles even before template lands**

Run: `cargo check`
Expected: handler compiles. Template render call may not yet error since template still references the old fields — we update it next.

> Note: if `cargo check` flags missing template fields, that's expected after Task 11 lands too — askama validates fields against the template at compile time. Move on; Task 11 will reconcile.

---

### Task 11: Update template — Progress Trend card

**Files:**
- Modify: `templates/stats/exercise.html`

- [ ] **Step 1: Replace the file contents**

Overwrite `templates/stats/exercise.html` with the version below. The "Progress Trend" card sits between "Personal Record" and "Recent History". The inline `<script>` is wrapped in an IIFE.

```html
{% extends "base.html" %}

{% block title %}{{ exercise.name }} Stats - LiftLog{% endblock %}

{% block content %}
{% include "nav.html" %}

<main>
    <div class="page-header">
        <h1>{{ exercise.name }}</h1>
        <div class="subtitle">{{ exercise.category }}</div>
    </div>

    <h2>Personal Record</h2>
    {% match pr %}
    {% when Some with (p) %}
    <div class="card card-gold" style="margin-bottom: var(--sp-6); display: inline-block;">
        <div style="display: flex; align-items: baseline; gap: var(--sp-4);">
            <span class="stat-value" style="font-size: var(--font-4xl);">{{ p.value }}</span>
            <span class="text-secondary text-sm">kg &middot; {{ p.achieved_at.format("%Y-%m-%d") }}</span>
        </div>
    </div>
    {% when None %}
    <p class="muted">No PR for this exercise yet.</p>
    {% endmatch %}

    <h2>Progress Trend</h2>
    <div class="card" style="margin-bottom: var(--sp-6);">
        {% if session_count == 0 %}
        <p class="muted">No progress data yet — log this exercise to see your trend.</p>
        {% else %}
        <div id="exercise-chart-controls" style="display: flex; gap: var(--sp-4); flex-wrap: wrap; margin-bottom: var(--sp-4);">
            <div role="group" aria-label="Metric" style="display: flex; gap: var(--sp-2);">
                <button type="button" class="btn btn-sm btn-tab is-active" data-metric="top_set">Top Set</button>
                <button type="button" class="btn btn-sm btn-tab" data-metric="e1rm">e1RM</button>
                <button type="button" class="btn btn-sm btn-tab" data-metric="volume">Volume</button>
            </div>
            <div role="group" aria-label="Range" style="display: flex; gap: var(--sp-2);">
                <button type="button" class="btn btn-sm btn-tab is-active" data-range="20">Last 20</button>
                <button type="button" class="btn btn-sm btn-tab" data-range="all">All</button>
            </div>
        </div>

        <div id="exercise-chart-wrap" style="position: relative;">
            <svg id="exercise-chart" viewBox="0 0 600 220" width="100%" preserveAspectRatio="xMidYMid meet" role="img" aria-label="Progress line chart">
                {% match chart %}
                {% when Some with (c) %}
                <g id="chart-y-grid">
                    {% for tick in c.y_ticks %}
                    <line x1="{{ c.padding_left }}" x2="{{ c.width - c.padding_right }}"
                          y1="{{ tick.0 }}" y2="{{ tick.0 }}"
                          stroke="var(--border-light)" stroke-width="1" />
                    <text x="{{ c.padding_left - 6.0 }}" y="{{ tick.0 + 4.0 }}"
                          text-anchor="end" font-size="11" fill="var(--text-secondary)">{{ tick.1 }}</text>
                    {% endfor %}
                </g>
                <g id="chart-x-labels">
                    {% for tick in c.x_ticks %}
                    <text x="{{ tick.0 }}" y="{{ c.height - 8.0 }}"
                          text-anchor="middle" font-size="11" fill="var(--text-secondary)">{{ tick.1 }}</text>
                    {% endfor %}
                </g>
                <polyline id="chart-line"
                          fill="none" stroke="var(--accent)" stroke-width="2"
                          points="{{ c.polyline }}" />
                <g id="chart-dots">
                    {% for pt in c.points %}
                    <circle cx="{{ pt.x }}" cy="{{ pt.y }}" r="4"
                            class="{% if pt.is_pr %}ll-dot-pr{% else %}ll-dot{% endif %}"
                            fill="{% if pt.is_pr %}var(--gold){% else %}var(--accent){% endif %}"
                            stroke="var(--bg)" stroke-width="1.5" />
                    {% endfor %}
                </g>
                {% when None %}
                {% if session_count == 1 %}
                <circle cx="300" cy="110" r="5" fill="var(--accent)" stroke="var(--bg)" stroke-width="1.5" />
                {% endif %}
                {% endmatch %}
                <g id="chart-hit-areas"></g>
            </svg>

            <div id="exercise-chart-tooltip"
                 style="position: absolute; display: none; pointer-events: none;
                        background: var(--surface); border: 1px solid var(--border);
                        padding: 8px 10px; font-size: 12px; border-radius: 6px;
                        color: var(--text-primary); white-space: nowrap; z-index: 2;"></div>
        </div>

        {% if session_count == 1 %}
        <p class="muted" style="margin-top: var(--sp-4);">Need at least 2 sessions to draw a trend.</p>
        {% endif %}

        <div style="display: flex; gap: var(--sp-4); margin-top: var(--sp-3); font-size: 12px; color: var(--text-secondary);">
            <span><span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:var(--accent);vertical-align:middle;margin-right:4px;"></span>Session</span>
            <span><span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:var(--gold);vertical-align:middle;margin-right:4px;"></span>Running PR</span>
        </div>

        <script type="application/json" id="exercise-chart-data">{{ chart_data_json|safe }}</script>
        {% endif %}
    </div>

    <h2>Recent History</h2>
    {% if history.is_empty() %}
    <p class="muted">No history for this exercise.</p>
    {% else %}
    <table class="data-table">
        <thead>
            <tr>
                <th>Set</th>
                <th>Weight</th>
                <th>Reps</th>
                <th>RPE</th>
                <th></th>
            </tr>
        </thead>
        <tbody>
            {% for log in history %}
            <tr>
                <td>{{ log.set_number }}</td>
                <td style="color: var(--text-primary); font-weight: 500;">{{ log.weight }}</td>
                <td>{{ log.reps }}</td>
                <td>{% match log.rpe %}{% when Some with (r) %}{{ r }}{% when None %}-{% endmatch %}</td>
                <td>{% if log.is_pr %}<span class="pr-badge">PR</span>{% endif %}</td>
            </tr>
            {% endfor %}
        </tbody>
    </table>
    {% endif %}

    <a href="/exercises" class="back-link">&larr; Back to Exercises</a>
</main>

{% if session_count >= 2 %}
<script>
(function() {
    var dataEl = document.getElementById('exercise-chart-data');
    if (!dataEl) return;
    var raw;
    try { raw = JSON.parse(dataEl.textContent); } catch (e) { return; }
    if (!Array.isArray(raw) || raw.length < 2) return;

    var svg = document.getElementById('exercise-chart');
    var line = document.getElementById('chart-line');
    var dots = document.getElementById('chart-dots');
    var xLabels = document.getElementById('chart-x-labels');
    var yGrid = document.getElementById('chart-y-grid');
    var hitAreas = document.getElementById('chart-hit-areas');
    var tooltip = document.getElementById('exercise-chart-tooltip');
    var wrap = document.getElementById('exercise-chart-wrap');

    var W = 600, H = 220, PL = 44, PR_ = 12, PT = 14, PB = 28;
    var plotW = W - PL - PR_, plotH = H - PT - PB;

    var activeMetric = 'top_set';
    var activeRange = '20';

    function metricValue(p, m) {
        if (m === 'top_set') return p.top_weight;
        if (m === 'e1rm') return p.e1rm;
        return p.volume;
    }
    function metricLabel(m) {
        if (m === 'top_set') return 'Top Set (kg)';
        if (m === 'e1rm') return 'e1RM (kg)';
        return 'Volume (kg)';
    }

    function redraw() {
        var slice = activeRange === 'all' ? raw : raw.slice(Math.max(0, raw.length - 20));
        var n = slice.length;
        if (n < 2) {
            // Sparse fallback inside the same SVG: clear line/dots, hide tooltip.
            line.setAttribute('points', '');
            dots.innerHTML = '';
            xLabels.innerHTML = '';
            yGrid.innerHTML = '';
            hitAreas.innerHTML = '';
            tooltip.style.display = 'none';
            return;
        }

        var vals = slice.map(function(p) { return metricValue(p, activeMetric); });
        var mn = Math.min.apply(null, vals);
        var mx = Math.max.apply(null, vals);
        if (mx === mn) { mn -= 1; mx += 1; }
        var pad = (mx - mn) * 0.1;
        var yMin = mn - pad, yMax = mx + pad;

        // Y grid + labels (4 ticks)
        yGrid.innerHTML = '';
        for (var i = 0; i < 4; i++) {
            var frac = i / 3;
            var y = PT + frac * plotH;
            var v = yMax - frac * (yMax - yMin);
            yGrid.insertAdjacentHTML('beforeend',
                '<line x1="' + PL + '" x2="' + (W - PR_) + '" y1="' + y + '" y2="' + y + '" stroke="var(--border-light)" stroke-width="1"/>' +
                '<text x="' + (PL - 6) + '" y="' + (y + 4) + '" text-anchor="end" font-size="11" fill="var(--text-secondary)">' + v.toFixed(0) + '</text>');
        }

        // Polyline + dots + PR detection
        var pts = [];
        var dotsHtml = '';
        var hitHtml = '';
        var labelHtml = '';
        var running = -Infinity;
        var bandW = plotW / Math.max(1, n - 1);

        for (var j = 0; j < n; j++) {
            var p = slice[j];
            var x = PL + (j / (n - 1)) * plotW;
            var val = metricValue(p, activeMetric);
            var yy = PT + (1 - (val - yMin) / (yMax - yMin)) * plotH;
            pts.push(x.toFixed(2) + ',' + yy.toFixed(2));
            var isPr = val > running;
            if (isPr) running = val;
            var color = isPr ? 'var(--gold)' : 'var(--accent)';
            dotsHtml += '<circle cx="' + x + '" cy="' + yy + '" r="4" fill="' + color + '" stroke="var(--bg)" stroke-width="1.5"/>';
            // Invisible hit-area for tooltip
            var hx = j === 0 ? PL : x - bandW / 2;
            var hw = (j === 0 || j === n - 1) ? bandW / 2 : bandW;
            hitHtml += '<rect x="' + hx + '" y="' + PT + '" width="' + hw + '" height="' + plotH + '" fill="transparent" data-i="' + j + '" style="cursor:crosshair"/>';
        }
        line.setAttribute('points', pts.join(' '));
        dots.innerHTML = dotsHtml;
        hitAreas.innerHTML = hitHtml;

        // X labels (up to 5)
        var lc = Math.min(5, n);
        for (var k = 0; k < lc; k++) {
            var idx = Math.floor(k * (n - 1) / Math.max(1, lc - 1));
            var lx = PL + (idx / (n - 1)) * plotW;
            labelHtml += '<text x="' + lx + '" y="' + (H - 8) + '" text-anchor="middle" font-size="11" fill="var(--text-secondary)">' + slice[idx].date.substring(5) + '</text>';
        }
        xLabels.innerHTML = labelHtml;

        // Tooltip handlers
        Array.prototype.forEach.call(hitAreas.querySelectorAll('rect'), function(rect) {
            rect.addEventListener('pointerenter', function(e) { showTip(slice, parseInt(rect.getAttribute('data-i'), 10)); });
            rect.addEventListener('pointermove',  function(e) { showTip(slice, parseInt(rect.getAttribute('data-i'), 10)); });
            rect.addEventListener('pointerleave', function() { tooltip.style.display = 'none'; });
            rect.addEventListener('pointerdown',  function(e) { showTip(slice, parseInt(rect.getAttribute('data-i'), 10)); });
        });
    }

    function showTip(slice, i) {
        var p = slice[i];
        var rect = svg.getBoundingClientRect();
        var n = slice.length;
        var xPct = n === 1 ? 0.5 : i / (n - 1);
        var leftPx = PL + xPct * plotW;
        var leftCss = (leftPx / W) * rect.width;
        tooltip.innerHTML =
            '<div><strong>' + p.date + '</strong></div>' +
            '<div>Top: ' + p.top_weight + ' kg × ' + p.top_reps + '</div>' +
            '<div>e1RM: ' + p.e1rm.toFixed(1) + ' kg</div>' +
            '<div>Volume: ' + p.volume.toFixed(0) + ' kg</div>';
        tooltip.style.display = 'block';
        // Clamp horizontally to wrap.
        var wrapW = wrap.getBoundingClientRect().width;
        var tipW = tooltip.offsetWidth;
        var left = Math.max(0, Math.min(wrapW - tipW, leftCss - tipW / 2));
        tooltip.style.left = left + 'px';
        tooltip.style.top = '0px';
    }

    // Tab wiring
    var controls = document.getElementById('exercise-chart-controls');
    controls.addEventListener('click', function(e) {
        var t = e.target;
        if (t.dataset.metric) {
            activeMetric = t.dataset.metric;
            controls.querySelectorAll('[data-metric]').forEach(function(b) { b.classList.toggle('is-active', b === t); });
            redraw();
        } else if (t.dataset.range) {
            activeRange = t.dataset.range;
            controls.querySelectorAll('[data-range]').forEach(function(b) { b.classList.toggle('is-active', b === t); });
            redraw();
        }
    });

    redraw();
})();
</script>
{% endif %}
{% endblock %}
```

- [ ] **Step 2: Add minimal `.btn-tab` styling — only if it doesn't already exist**

Run: `grep -n "btn-tab\|is-active" templates/base.html`

If `.btn-tab` is already defined, skip this step. Otherwise, add to `templates/base.html` inside the existing `<style>` block, near the other `.btn` rules:

```css
.btn-tab {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-secondary);
}
.btn-tab.is-active {
    background: var(--accent-muted);
    border-color: var(--accent);
    color: var(--accent);
}
```

- [ ] **Step 3: Compile**

Run: `cargo check`
Expected: askama macro compiles the template without errors. If the askama macro complains about a missing field, the struct in Task 10 and the template above must agree exactly — recheck names.

- [ ] **Step 4: Format**

Run: `cargo fmt`

---

### Task 12: Integration test — chart renders with ≥ 2 sessions

**Files:**
- Modify: `tests/stats_test.rs`

- [ ] **Step 1: Write the failing test**

Append to the end of `tests/stats_test.rs`:

```rust
#[tokio::test]
async fn test_exercise_stats_chart_renders_with_two_or_more_sessions() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;

    for (i, weight) in [100.0_f64, 105.0, 110.0].iter().enumerate() {
        let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 10 + i as u32 * 2).unwrap();
        let workout = common::create_test_workout(&pool, &user.id, date, None).await;
        common::create_test_log(&pool, &workout.id, &exercise.id, 1, 5, *weight, None).await;
    }

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(&format!("/stats/exercise/{}", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Server-rendered SVG line is present
    assert!(body_str.contains("<polyline"), "polyline missing");
    assert!(body_str.contains("id=\"chart-line\""));

    // JSON-embedded dataset is present and parseable
    let start = body_str
        .find("id=\"exercise-chart-data\">")
        .expect("chart-data script tag missing");
    let after_open = &body_str[start + "id=\"exercise-chart-data\">".len()..];
    let end = after_open.find("</script>").expect("chart-data script close tag");
    let json_text = &after_open[..end];
    let parsed: serde_json::Value =
        serde_json::from_str(json_text).expect("chart data JSON should parse");
    let arr = parsed.as_array().expect("chart data should be an array");
    assert_eq!(arr.len(), 3);
    let first = &arr[0];
    assert!(first.get("top_weight").is_some());
    assert!(first.get("top_reps").is_some());
    assert!(first.get("volume").is_some());
    assert!(first.get("e1rm").is_some());
    assert!(first.get("date").is_some());
}
```

- [ ] **Step 2: Add the `serde_json` dev-dependency hook if needed**

The integration test parses JSON. `serde_json` is a runtime dep (Task 1) so it's available to the integration test crate without additional configuration. Confirm by running `cargo check --tests`. Expected: clean.

- [ ] **Step 3: Run the test**

Run: `cargo nextest run --test stats_test test_exercise_stats_chart_renders_with_two_or_more_sessions`
Expected: PASS.

---

### Task 13: Integration test — sparse state (1 session)

**Files:**
- Modify: `tests/stats_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_exercise_stats_chart_renders_sparse_state_with_one_session() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
    let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let workout = common::create_test_workout(&pool, &user.id, date, None).await;
    common::create_test_log(&pool, &workout.id, &exercise.id, 1, 5, 100.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(&format!("/stats/exercise/{}", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    assert!(body_str.contains("Need at least 2 sessions"));
    assert!(!body_str.contains("<polyline"));
}
```

- [ ] **Step 2: Run it**

Run: `cargo nextest run --test stats_test test_exercise_stats_chart_renders_sparse_state_with_one_session`
Expected: PASS.

---

### Task 14: Integration test — empty state (0 sessions)

**Files:**
- Modify: `tests/stats_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_exercise_stats_chart_renders_empty_state_with_no_logs() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(&format!("/stats/exercise/{}", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    assert!(body_str.contains("No progress data yet"));
    assert!(!body_str.contains("<polyline"));
    assert!(!body_str.contains("id=\"exercise-chart-data\""));
}
```

- [ ] **Step 2: Run it**

Run: `cargo nextest run --test stats_test test_exercise_stats_chart_renders_empty_state_with_no_logs`
Expected: PASS.

---

### Task 15: Integration test — PR dots match running-max indices

**Files:**
- Modify: `tests/stats_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_exercise_stats_chart_pr_dots_match_expected_indices() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;

    // Weights ASC: [100, 100, 110, 105, 120]
    // Running max PRs at indices 0, 2, 4 (the first 100 also counts as the first running max).
    let weights: [f64; 5] = [100.0, 100.0, 110.0, 105.0, 120.0];
    for (i, w) in weights.iter().enumerate() {
        let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 10 + i as u32).unwrap();
        let workout = common::create_test_workout(&pool, &user.id, date, None).await;
        common::create_test_log(&pool, &workout.id, &exercise.id, 1, 5, *w, None).await;
    }

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(&format!("/stats/exercise/{}", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // 5 dots total — running PRs are at index 0 (100), 2 (110), 4 (120).
    let pr_count = body_str.matches("class=\"ll-dot-pr\"").count();
    let plain_count = body_str.matches("class=\"ll-dot\"").count();
    assert_eq!(pr_count, 3, "expected 3 PR dots, body=\n{}", body_str);
    assert_eq!(plain_count, 2);
}
```

- [ ] **Step 2: Run it**

Run: `cargo nextest run --test stats_test test_exercise_stats_chart_pr_dots_match_expected_indices`
Expected: PASS.

> **If the strict-equal assertion misfires** because the template renders the class attribute differently (e.g. extra whitespace, `class='ll-dot'` with single quotes), match the literal string used by the template *exactly* — do not loosen the assertion to "contains some marker".

---

### Task 16: Phase B — full test run, format check, commit

**Files:**
- All Phase B files.

- [ ] **Step 1: Run full test suite**

Run: `cargo nextest run`
Expected: every test passes — including all repo tests from Phase A, all new integration tests, and pre-existing tests.

- [ ] **Step 2: Format and verify**

Run: `cargo fmt`
Run: `cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 3: Verify the working tree contains only intended changes**

Run: `git status`
Expected modified set: `src/handlers/stats.rs`, `templates/stats/exercise.html`, `tests/stats_test.rs`, optionally `templates/base.html` (only if Task 11 step 2 added `.btn-tab`).

- [ ] **Step 4: Commit Phase B**

```bash
git add src/handlers/stats.rs templates/stats/exercise.html tests/stats_test.rs
# only if base.html was actually edited:
# git add templates/base.html
git commit -S -m "feat(stats): render progress line chart on per-exercise page"
```

Run `git log -2 --oneline` to confirm both Phase A and Phase B commits are present.

---

### Task 17: Manual browser smoke test

**Files:**
- None — this is exploratory.

- [ ] **Step 1: Start the dev server**

Run (in a separate terminal or via `run_in_background`): `cargo run`
Expected: server logs `Listening on …`. Default is `127.0.0.1:3000`.

- [ ] **Step 2: Log in and navigate**

Open `http://127.0.0.1:3000/auth/login`, log in, then visit `/stats/exercise/<id>` for an exercise with ≥ 2 logged sessions.

Verify:
- Server-rendered SVG visible **before JS runs** (disable JS in devtools and reload — line + dots still appear).
- "Top Set / e1RM / Volume" tabs swap the line in place without a network request (Network tab stays quiet).
- "Last 20 / All" tabs change the slice; Y-axis rescales.
- Hover/tap a dot → tooltip shows date, top weight × reps, e1RM, volume.
- A PR (running-max) session shows a gold dot. Subsequent non-PR sessions show accent-color dots.
- For an exercise with exactly 1 session → "Need at least 2 sessions" copy + single dot.
- For an exercise with 0 logs → "No progress data yet" copy.

- [ ] **Step 3: Stop the server**

Stop the `cargo run` process.

---

## Self-Review Checklist

**Spec coverage:**
- Per-exercise placement only ✅ Tasks 10–11 (template only on `/stats/exercise/{id}`).
- Top set default + e1RM/volume tabs ✅ Task 11 inline JS + Task 10 default render.
- Last 20 default + All toggle ✅ Task 10 default + Task 11 client switch.
- Equal-spaced session ordering ✅ Task 10 `i / (n-1)` mapping.
- Gold dots on running-max PR ✅ Task 10 `is_pr` + Task 15 test.
- Empty / sparse / full states ✅ Tasks 13/14/12 cover all three.
- Inline SVG default + JSON embed + vanilla JS ✅ Tasks 10–11.
- Top-set tie-break by reps ✅ Task 8 SQL + test.
- 6 repo unit tests ✅ Tasks 3–8.
- 4 integration tests ✅ Tasks 12–15.
- Two commits, both green ✅ Tasks 9 + 16.

**Placeholder scan:** None present — every code step contains the actual code; commands have expected output; no "TBD"/"similar to". The one judgment call (`AppError` variant name) gives a concrete fallback in Task 10 step 2.

**Type consistency:** `ChartPoint`, `RenderedChart`, `RenderedPoint`, `chart_data_json`, `session_count` are defined once in Task 10 and used identically in Task 11's template and Task 12's integration test. `ll-dot` / `ll-dot-pr` class names match between Task 11 (template) and Task 15 (assertion). SVG IDs (`exercise-chart`, `chart-line`, `chart-dots`, `chart-x-labels`, `chart-y-grid`, `chart-hit-areas`, `exercise-chart-tooltip`, `exercise-chart-controls`, `exercise-chart-data`) are consistent between template and JS.
