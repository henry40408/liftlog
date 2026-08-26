use chrono::{DateTime, Utc};
use rusqlite::Row;
use serde::{Deserialize, Deserializer, Serialize};

use super::FromSqliteRow;

/// Empty strings deserialize to `None` rather than failing.
fn deserialize_optional_i32<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => s.parse().map(Some).map_err(serde::de::Error::custom),
        None => Ok(None),
    }
}

/// The rep counts worth offering as suggestions on the Add Set form, in the
/// order a browser renders them: singles and triples for strength work, 5 and
/// 6 for the 5x5 family, 8-12 for hypertrophy, 15 and 20 for endurance.
///
/// Code, not data, for the same reason `CATEGORIES` is: this is a vocabulary
/// the application asserts, not something a user configures. And a suggestion
/// only — the field still accepts any positive integer, exactly as it did
/// before, because programmes exist that do none of these.
pub const REP_SCHEMES: &[i32] = &[1, 3, 5, 6, 8, 10, 12, 15, 20];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkoutLog {
    pub id: String,
    pub session_id: String,
    pub exercise_id: String,
    pub set_number: i32,
    pub reps: i32,
    pub weight: f64,
    pub rpe: Option<i32>,
    pub created_at: DateTime<Utc>,
}

impl FromSqliteRow for WorkoutLog {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            session_id: row.get("session_id")?,
            exercise_id: row.get("exercise_id")?,
            set_number: row.get("set_number")?,
            reps: row.get("reps")?,
            weight: row.get("weight")?,
            rpe: row.get("rpe")?,
            created_at: row.get("created_at")?,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkoutLog {
    pub exercise_id: String,
    pub reps: i32,
    pub weight: f64,
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub rpe: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkoutLog {
    pub reps: i32,
    pub weight: f64,
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub rpe: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkoutLogWithExercise {
    pub id: String,
    pub session_id: String,
    pub exercise_id: String,
    pub exercise_name: String,
    pub set_number: i32,
    pub reps: i32,
    pub weight: f64,
    pub rpe: Option<i32>,
    /// The set matches the all-time best weight for its exercise.
    pub is_pr: bool,
    /// The set is inside the rolling 1-month window *and* matches the best
    /// weight logged for its exercise within it. An all-time PR logged today
    /// sets both flags; one from two years ago sets only `is_pr`.
    pub is_recent_pr: bool,
}

impl FromSqliteRow for WorkoutLogWithExercise {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            session_id: row.get("session_id")?,
            exercise_id: row.get("exercise_id")?,
            exercise_name: row.get("exercise_name")?,
            set_number: row.get("set_number")?,
            reps: row.get("reps")?,
            weight: row.get("weight")?,
            rpe: row.get("rpe")?,
            is_pr: row.get("is_pr")?,
            is_recent_pr: row.get("is_recent_pr")?,
        })
    }
}
