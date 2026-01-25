use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Exercise {
    pub id: String,
    pub name: String,
    pub category: String,
    pub muscle_group: String,
    pub equipment: Option<String>,
    pub is_default: bool,
    pub user_id: Option<String>,
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
    ExerciseCategory { name: "chest", display_name: "Chest" },
    ExerciseCategory { name: "back", display_name: "Back" },
    ExerciseCategory { name: "legs", display_name: "Legs" },
    ExerciseCategory { name: "shoulders", display_name: "Shoulders" },
    ExerciseCategory { name: "arms", display_name: "Arms" },
    ExerciseCategory { name: "core", display_name: "Core" },
];
