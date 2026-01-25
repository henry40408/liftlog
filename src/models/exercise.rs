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
    ExerciseCategory { name: "chest", display_name: "胸" },
    ExerciseCategory { name: "back", display_name: "背" },
    ExerciseCategory { name: "legs", display_name: "腿" },
    ExerciseCategory { name: "shoulders", display_name: "肩" },
    ExerciseCategory { name: "arms", display_name: "手臂" },
    ExerciseCategory { name: "core", display_name: "核心" },
];
