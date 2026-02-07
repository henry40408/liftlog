use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::Row;
use serde::{Deserialize, Serialize};

use super::FromSqliteRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkoutSession {
    pub id: String,
    pub user_id: String,
    pub date: NaiveDate,
    pub notes: Option<String>,
    pub share_token: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl FromSqliteRow for WorkoutSession {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            user_id: row.get("user_id")?,
            date: row.get("date")?,
            notes: row.get("notes")?,
            share_token: row.get("share_token")?,
            created_at: row.get("created_at")?,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkoutSession {
    pub date: NaiveDate,
    pub notes: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct UpdateWorkoutSession {
    pub date: Option<NaiveDate>,
    pub notes: Option<String>,
}
