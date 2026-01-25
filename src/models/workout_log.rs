use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WorkoutLog {
    pub id: String,
    pub session_id: String,
    pub exercise_id: String,
    pub set_number: i32,
    pub reps: i32,
    pub weight: f64,
    pub rpe: Option<i32>,
    pub is_pr: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkoutLog {
    pub exercise_id: String,
    pub set_number: i32,
    pub reps: i32,
    pub weight: f64,
    pub rpe: Option<i32>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
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
