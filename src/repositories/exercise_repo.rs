use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::Result;
use crate::models::Exercise;

#[derive(Clone)]
pub struct ExerciseRepository {
    pool: SqlitePool,
}

impl ExerciseRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Exercise>> {
        let exercise = sqlx::query_as::<_, Exercise>("SELECT * FROM exercises WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(exercise)
    }

    pub async fn find_all(&self) -> Result<Vec<Exercise>> {
        let exercises = sqlx::query_as::<_, Exercise>(
            "SELECT * FROM exercises ORDER BY category, name"
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(exercises)
    }

    pub async fn find_by_category(&self, category: &str) -> Result<Vec<Exercise>> {
        let exercises = sqlx::query_as::<_, Exercise>(
            "SELECT * FROM exercises WHERE category = ? ORDER BY name"
        )
        .bind(category)
        .fetch_all(&self.pool)
        .await?;
        Ok(exercises)
    }

    pub async fn find_available_for_user(&self, user_id: &str) -> Result<Vec<Exercise>> {
        let exercises = sqlx::query_as::<_, Exercise>(
            "SELECT * FROM exercises WHERE is_default = 1 OR user_id = ? ORDER BY category, name"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(exercises)
    }

    pub async fn find_user_custom(&self, user_id: &str) -> Result<Vec<Exercise>> {
        let exercises = sqlx::query_as::<_, Exercise>(
            "SELECT * FROM exercises WHERE user_id = ? ORDER BY category, name"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(exercises)
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

        sqlx::query(
            "INSERT INTO exercises (id, name, category, muscle_group, equipment, is_default, user_id)
             VALUES (?, ?, ?, ?, ?, 0, ?)"
        )
        .bind(&id)
        .bind(name)
        .bind(category)
        .bind(muscle_group)
        .bind(equipment)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(Exercise {
            id,
            name: name.to_string(),
            category: category.to_string(),
            muscle_group: muscle_group.to_string(),
            equipment: equipment.map(|s| s.to_string()),
            is_default: false,
            user_id: Some(user_id.to_string()),
        })
    }

    pub async fn delete(&self, id: &str, user_id: &str) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM exercises WHERE id = ? AND user_id = ? AND is_default = 0"
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
