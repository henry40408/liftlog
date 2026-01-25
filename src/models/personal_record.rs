use chrono::{DateTime, Utc};
use rusqlite::Row;
use serde::{Deserialize, Serialize};

use super::FromSqliteRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalRecord {
    pub id: String,
    pub user_id: String,
    pub exercise_id: String,
    pub record_type: String,
    pub value: f64,
    pub achieved_at: DateTime<Utc>,
}

impl FromSqliteRow for PersonalRecord {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            user_id: row.get("user_id")?,
            exercise_id: row.get("exercise_id")?,
            record_type: row.get("record_type")?,
            value: row.get("value")?,
            achieved_at: row.get("achieved_at")?,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonalRecordWithExercise {
    pub id: String,
    pub user_id: String,
    pub exercise_id: String,
    pub exercise_name: String,
    pub record_type: String,
    pub value: f64,
    pub achieved_at: DateTime<Utc>,
}

impl FromSqliteRow for PersonalRecordWithExercise {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            user_id: row.get("user_id")?,
            exercise_id: row.get("exercise_id")?,
            exercise_name: row.get("exercise_name")?,
            record_type: row.get("record_type")?,
            value: row.get("value")?,
            achieved_at: row.get("achieved_at")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordType {
    MaxWeight,
    OneRepMax,
    FiveRepMax,
}

impl RecordType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RecordType::MaxWeight => "max_weight",
            RecordType::OneRepMax => "1rm",
            RecordType::FiveRepMax => "5rm",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            RecordType::MaxWeight => "Max Weight",
            RecordType::OneRepMax => "1RM",
            RecordType::FiveRepMax => "5RM",
        }
    }
}
