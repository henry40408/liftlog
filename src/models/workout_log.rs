use chrono::{DateTime, Utc};
use rusqlite::Row;
use serde::{Deserialize, Deserializer, Serialize};

use super::FromSqliteRow;

/// Deserialize an optional integer from a form field.
/// Handles empty strings by returning None instead of failing.
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
    pub is_pr: bool,
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
        })
    }
}
