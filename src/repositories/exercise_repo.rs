use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, Result};
use crate::models::{Exercise, FromSqliteRow};

#[derive(Clone)]
pub struct ExerciseRepository {
    pool: DbPool,
}

impl ExerciseRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Exercise>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare("SELECT * FROM exercises WHERE id = ?")?;
            let result = stmt.query_row([&id], Exercise::from_row).optional()?;
            Ok(result)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    #[allow(dead_code)]
    pub async fn find_all(&self) -> Result<Vec<Exercise>> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare("SELECT * FROM exercises ORDER BY category, name")?;
            let exercises = stmt
                .query_map([], Exercise::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(exercises)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    #[allow(dead_code)]
    pub async fn find_by_category(&self, category: &str) -> Result<Vec<Exercise>> {
        let pool = self.pool.clone();
        let category = category.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt =
                conn.prepare("SELECT * FROM exercises WHERE category = ? ORDER BY name")?;
            let exercises = stmt
                .query_map([&category], Exercise::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(exercises)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn find_available_for_user(&self, user_id: &str) -> Result<Vec<Exercise>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT * FROM exercises WHERE is_default = 1 OR user_id = ? ORDER BY category, name"
            )?;
            let exercises = stmt
                .query_map([&user_id], Exercise::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(exercises)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    #[allow(dead_code)]
    pub async fn find_user_custom(&self, user_id: &str) -> Result<Vec<Exercise>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt =
                conn.prepare("SELECT * FROM exercises WHERE user_id = ? ORDER BY category, name")?;
            let exercises = stmt
                .query_map([&user_id], Exercise::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(exercises)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn create(
        &self,
        name: &str,
        category: &str,
        muscle_group: &str,
        equipment: Option<&str>,
        user_id: &str,
    ) -> Result<Exercise> {
        let id = Uuid::new_v4().to_string();
        let exercise = Exercise {
            id: id.clone(),
            name: name.to_string(),
            category: category.to_string(),
            muscle_group: muscle_group.to_string(),
            equipment: equipment.map(|s| s.to_string()),
            is_default: false,
            user_id: Some(user_id.to_string()),
        };
        let exercise_clone = exercise.clone();

        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO exercises (id, name, category, muscle_group, equipment, is_default, user_id)
                 VALUES (?, ?, ?, ?, ?, 0, ?)",
                rusqlite::params![
                    exercise_clone.id,
                    exercise_clone.name,
                    exercise_clone.category,
                    exercise_clone.muscle_group,
                    exercise_clone.equipment,
                    exercise_clone.user_id
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;

        Ok(exercise)
    }

    #[allow(dead_code)]
    pub async fn delete(&self, id: &str, user_id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let rows = conn.execute(
                "DELETE FROM exercises WHERE id = ? AND user_id = ? AND is_default = 0",
                rusqlite::params![id, user_id],
            )?;
            Ok(rows > 0)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }
}
