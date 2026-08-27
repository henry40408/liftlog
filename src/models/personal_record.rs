use chrono::{DateTime, Duration, Utc};
use rusqlite::Row;
use serde::Serialize;

use super::FromSqliteRow;

/// Width of the "PR (1M)" window. A rolling 30 days, not a calendar month, so
/// the number never resets just because a new month started.
pub const RECENT_PR_WINDOW_DAYS: i64 = 30;

/// Start of the rolling window every "PR (1M)" surface is measured against —
/// the PR tables, and the per-set badges on a workout.
pub fn recent_pr_window_start() -> DateTime<Utc> {
    Utc::now() - Duration::days(RECENT_PR_WINDOW_DAYS)
}

/// Dynamically computed Personal Record
#[derive(Debug, Clone, Serialize)]
pub struct DynamicPR {
    pub exercise_id: String,
    pub exercise_name: String,
    pub value: f64,
    pub achieved_at: DateTime<Utc>,
}

impl FromSqliteRow for DynamicPR {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            exercise_id: row.get("exercise_id")?,
            exercise_name: row.get("exercise_name")?,
            value: row.get("value")?,
            achieved_at: row.get("achieved_at")?,
        })
    }
}

/// A single exercise's personal record over two windows: all-time, and a
/// rolling recent window (see `WorkoutRepository::get_pr_summaries_by_user`).
///
/// The recent fields are `None` when the exercise has no logs inside the
/// window — an exercise last trained a year ago still has an all-time PR.
#[derive(Debug, Clone, Serialize)]
pub struct PersonalRecordSummary {
    pub exercise_id: String,
    pub exercise_name: String,
    pub all_time_value: f64,
    pub all_time_achieved_at: DateTime<Utc>,
    pub recent_value: Option<f64>,
    pub recent_achieved_at: Option<DateTime<Utc>>,
}

impl PersonalRecordSummary {
    /// True when the recent-window best *is* the all-time best — the all-time
    /// PR was set inside the window. Templates use it to keep the highlight on
    /// the recent column in that case.
    pub fn recent_is_all_time(&self) -> bool {
        self.recent_value.is_some_and(|v| v >= self.all_time_value)
    }
}

impl FromSqliteRow for PersonalRecordSummary {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            exercise_id: row.get("exercise_id")?,
            exercise_name: row.get("exercise_name")?,
            all_time_value: row.get("all_time_value")?,
            all_time_achieved_at: row.get("all_time_achieved_at")?,
            recent_value: row.get("recent_value")?,
            recent_achieved_at: row.get("recent_achieved_at")?,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LastExerciseWeight {
    pub exercise_id: String,
    pub weight: f64,
    pub rpe: Option<i32>,
    pub logged_at: DateTime<Utc>,
}

impl FromSqliteRow for LastExerciseWeight {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            exercise_id: row.get("exercise_id")?,
            weight: row.get("weight")?,
            rpe: row.get("rpe")?,
            logged_at: row.get("logged_at")?,
        })
    }
}
