use chrono::{DateTime, Utc};
use rusqlite::Row;
use serde::Serialize;

use super::FromSqliteRow;

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
