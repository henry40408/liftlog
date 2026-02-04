use rusqlite::Row;
use serde::{Deserialize, Serialize};

use super::FromSqliteRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exercise {
    pub id: String,
    pub name: String,
    pub category: String,
    pub user_id: String,
}

impl FromSqliteRow for Exercise {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            category: row.get("category")?,
            user_id: row.get("user_id")?,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateExercise {
    pub name: String,
    pub category: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateExercise {
    pub name: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExerciseCategory {
    pub name: &'static str,
    pub display_name: &'static str,
}

pub const CATEGORIES: &[ExerciseCategory] = &[
    ExerciseCategory {
        name: "chest",
        display_name: "Chest",
    },
    ExerciseCategory {
        name: "back",
        display_name: "Back",
    },
    ExerciseCategory {
        name: "legs",
        display_name: "Legs",
    },
    ExerciseCategory {
        name: "shoulders",
        display_name: "Shoulders",
    },
    ExerciseCategory {
        name: "arms",
        display_name: "Arms",
    },
    ExerciseCategory {
        name: "core",
        display_name: "Core",
    },
];
