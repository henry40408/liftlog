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

    pub async fn create(&self, name: &str, category: &str, user_id: &str) -> Result<Exercise> {
        let id = Uuid::new_v4().to_string();
        let exercise = Exercise {
            id: id.clone(),
            name: name.to_string(),
            category: category.to_string(),
            user_id: user_id.to_string(),
        };
        let exercise_clone = exercise.clone();

        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO exercises (id, name, category, user_id)
                 VALUES (?, ?, ?, ?)",
                rusqlite::params![
                    exercise_clone.id,
                    exercise_clone.name,
                    exercise_clone.category,
                    exercise_clone.user_id
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;

        Ok(exercise)
    }

    pub async fn update(
        &self,
        id: &str,
        user_id: &str,
        name: &str,
        category: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let user_id = user_id.to_string();
        let name = name.to_string();
        let category = category.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let rows = conn.execute(
                "UPDATE exercises SET name = ?, category = ? WHERE id = ? AND user_id = ?",
                rusqlite::params![name, category, id, user_id],
            )?;
            Ok(rows > 0)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn delete(&self, id: &str, user_id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let user_id = user_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let rows = conn.execute(
                "DELETE FROM exercises WHERE id = ? AND user_id = ?",
                rusqlite::params![id, user_id],
            )?;
            Ok(rows > 0)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_memory_pool;
    use crate::migrations::run_migrations_for_tests;

    fn setup_test_db() -> DbPool {
        let pool = create_memory_pool().expect("Failed to create test database");
        run_migrations_for_tests(&pool).expect("Failed to run migrations");
        pool
    }

    fn create_test_user(pool: &DbPool, user_id: &str) {
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?, ?, ?, ?, datetime('now'))",
            rusqlite::params![user_id, format!("user_{}", user_id), "hash", "user"],
        ).unwrap();
    }

    #[tokio::test]
    async fn test_create_exercise() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        let repo = ExerciseRepository::new(pool);

        let exercise = repo.create("Bench Press", "chest", "user1").await.unwrap();

        assert_eq!(exercise.name, "Bench Press");
        assert_eq!(exercise.category, "chest");
        assert_eq!(exercise.user_id, "user1");
        assert!(!exercise.id.is_empty());
    }

    #[tokio::test]
    async fn test_find_by_id_exists() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        let repo = ExerciseRepository::new(pool);

        let created = repo.create("Bench Press", "chest", "user1").await.unwrap();
        let found = repo.find_by_id(&created.id).await.unwrap();

        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, created.id);
        assert_eq!(found.name, "Bench Press");
    }

    #[tokio::test]
    async fn test_find_by_id_not_exists() {
        let pool = setup_test_db();
        let repo = ExerciseRepository::new(pool);

        let found = repo.find_by_id("nonexistent").await.unwrap();

        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_available_for_user() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_user(&pool, "user2");
        let repo = ExerciseRepository::new(pool);

        repo.create("Bench Press", "chest", "user1").await.unwrap();
        repo.create("Squat", "legs", "user1").await.unwrap();
        repo.create("Deadlift", "back", "user2").await.unwrap();

        let user1_exercises = repo.find_available_for_user("user1").await.unwrap();
        let user2_exercises = repo.find_available_for_user("user2").await.unwrap();

        assert_eq!(user1_exercises.len(), 2);
        assert_eq!(user2_exercises.len(), 1);
    }

    #[tokio::test]
    async fn test_update_success() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        let repo = ExerciseRepository::new(pool);

        let exercise = repo.create("Bench Press", "chest", "user1").await.unwrap();
        let updated = repo
            .update(&exercise.id, "user1", "Incline Bench", "chest")
            .await
            .unwrap();

        assert!(updated);

        let found = repo.find_by_id(&exercise.id).await.unwrap().unwrap();
        assert_eq!(found.name, "Incline Bench");
    }

    #[tokio::test]
    async fn test_update_wrong_user() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_user(&pool, "user2");
        let repo = ExerciseRepository::new(pool);

        let exercise = repo.create("Bench Press", "chest", "user1").await.unwrap();
        let updated = repo
            .update(&exercise.id, "user2", "Hacked", "chest")
            .await
            .unwrap();

        assert!(!updated);

        // Verify exercise was not modified
        let found = repo.find_by_id(&exercise.id).await.unwrap().unwrap();
        assert_eq!(found.name, "Bench Press");
    }

    #[tokio::test]
    async fn test_delete_success() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        let repo = ExerciseRepository::new(pool);

        let exercise = repo.create("Bench Press", "chest", "user1").await.unwrap();
        let deleted = repo.delete(&exercise.id, "user1").await.unwrap();

        assert!(deleted);

        let found = repo.find_by_id(&exercise.id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_delete_wrong_user() {
        let pool = setup_test_db();
        create_test_user(&pool, "user1");
        create_test_user(&pool, "user2");
        let repo = ExerciseRepository::new(pool);

        let exercise = repo.create("Bench Press", "chest", "user1").await.unwrap();
        let deleted = repo.delete(&exercise.id, "user2").await.unwrap();

        assert!(!deleted);

        // Verify exercise was not deleted
        let found = repo.find_by_id(&exercise.id).await.unwrap();
        assert!(found.is_some());
    }
}
