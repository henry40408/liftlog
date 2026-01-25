use rusqlite::Row;
use serde::{Deserialize, Serialize};

use super::FromSqliteRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exercise {
    pub id: String,
    pub name: String,
    pub category: String,
    pub muscle_group: String,
    pub equipment: Option<String>,
    pub is_default: bool,
    pub user_id: Option<String>,
}

impl FromSqliteRow for Exercise {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            category: row.get("category")?,
            muscle_group: row.get("muscle_group")?,
            equipment: row.get("equipment")?,
            is_default: row.get("is_default")?,
            user_id: row.get("user_id")?,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateExercise {
    pub name: String,
    pub category: String,
    pub muscle_group: String,
    pub equipment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExerciseCategory {
    pub name: &'static str,
    pub display_name: &'static str,
}

pub const CATEGORIES: &[ExerciseCategory] = &[
    ExerciseCategory { name: "chest", display_name: "胸" },
    ExerciseCategory { name: "back", display_name: "背" },
    ExerciseCategory { name: "legs", display_name: "腿" },
    ExerciseCategory { name: "shoulders", display_name: "肩" },
    ExerciseCategory { name: "arms", display_name: "手臂" },
    ExerciseCategory { name: "core", display_name: "核心" },
];
