use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PersonalRecord {
    pub id: String,
    pub user_id: String,
    pub exercise_id: String,
    pub record_type: String,
    pub value: f64,
    pub achieved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PersonalRecordWithExercise {
    pub id: String,
    pub user_id: String,
    pub exercise_id: String,
    pub exercise_name: String,
    pub record_type: String,
    pub value: f64,
    pub achieved_at: DateTime<Utc>,
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
