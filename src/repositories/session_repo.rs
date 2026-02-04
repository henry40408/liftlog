use chrono::Utc;
use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, Result};

#[derive(Clone)]
pub struct SessionRepository {
    pool: DbPool,
}

impl SessionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Create a new session for a user. Returns the session token.
    pub async fn create(&self, user_id: &str) -> Result<String> {
        let pool = self.pool.clone();
        let token = Uuid::new_v4().to_string();
        let user_id = user_id.to_string();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::days(7);
        let token_clone = token.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO sessions (token, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
                rusqlite::params![token_clone, user_id, now, expires_at],
            )?;
            Ok(token_clone)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    /// Find a valid (non-expired) session and return its user_id.
    /// Lazily deletes the session if it has expired.
    pub async fn find_valid(&self, token: &str) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let token = token.to_string();
        let now = Utc::now();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let result: Option<(String, chrono::DateTime<Utc>)> = conn
                .query_row(
                    "SELECT user_id, expires_at FROM sessions WHERE token = ?",
                    [&token],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            match result {
                Some((user_id, expires_at)) => {
                    if expires_at <= now {
                        // Lazily delete expired session
                        conn.execute("DELETE FROM sessions WHERE token = ?", [&token])?;
                        Ok(None)
                    } else {
                        Ok(Some(user_id))
                    }
                }
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    /// Delete a single session (logout).
    pub async fn delete(&self, token: &str) -> Result<()> {
        let pool = self.pool.clone();
        let token = token.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute("DELETE FROM sessions WHERE token = ?", [&token])?;
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    /// Delete all sessions for a user except the given token (for password change).
    pub async fn delete_all_for_user_except(&self, user_id: &str, keep_token: &str) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let keep_token = keep_token.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute(
                "DELETE FROM sessions WHERE user_id = ? AND token != ?",
                rusqlite::params![user_id, keep_token],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    /// Batch delete all expired sessions.
    pub async fn cleanup_expired(&self) -> Result<()> {
        let pool = self.pool.clone();
        let now = Utc::now();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute(
                "DELETE FROM sessions WHERE expires_at <= ?",
                rusqlite::params![now],
            )?;
            Ok(())
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
    use crate::models::UserRole;
    use crate::repositories::UserRepository;

    fn setup_test_db() -> crate::db::DbPool {
        let pool = create_memory_pool().expect("Failed to create test database");
        run_migrations_for_tests(&pool).expect("Failed to run migrations");
        pool
    }

    async fn create_user(pool: &crate::db::DbPool) -> String {
        let user_repo = UserRepository::new(pool.clone());
        let user = user_repo
            .create("testuser", "password", UserRole::User)
            .await
            .unwrap();
        user.id
    }

    #[tokio::test]
    async fn test_create_and_find_valid() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool);

        let token = repo.create(&user_id).await.unwrap();
        assert!(!token.is_empty());

        let found = repo.find_valid(&token).await.unwrap();
        assert_eq!(found, Some(user_id));
    }

    #[tokio::test]
    async fn test_find_valid_nonexistent() {
        let pool = setup_test_db();
        let repo = SessionRepository::new(pool);

        let found = repo.find_valid("nonexistent-token").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_valid_expired() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token = repo.create(&user_id).await.unwrap();

        // Manually expire the session (scoped to release connection before await)
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
                [&token],
            )
            .unwrap();
        }

        // Should return None and lazily delete
        let found = repo.find_valid(&token).await.unwrap();
        assert!(found.is_none());

        // Should be deleted from DB
        {
            let conn = pool.get().unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sessions WHERE token = ?",
                    [&token],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
        }
    }

    #[tokio::test]
    async fn test_delete() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool);

        let token = repo.create(&user_id).await.unwrap();
        repo.delete(&token).await.unwrap();

        let found = repo.find_valid(&token).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_delete_all_for_user_except() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool);

        let token1 = repo.create(&user_id).await.unwrap();
        let token2 = repo.create(&user_id).await.unwrap();
        let token3 = repo.create(&user_id).await.unwrap();

        // Keep token2, delete the rest
        repo.delete_all_for_user_except(&user_id, &token2)
            .await
            .unwrap();

        assert!(repo.find_valid(&token1).await.unwrap().is_none());
        assert!(repo.find_valid(&token2).await.unwrap().is_some());
        assert!(repo.find_valid(&token3).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token_valid = repo.create(&user_id).await.unwrap();
        let token_expired = repo.create(&user_id).await.unwrap();

        // Manually expire one session (scoped to release connection before await)
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
                [&token_expired],
            )
            .unwrap();
        }

        repo.cleanup_expired().await.unwrap();

        // Valid session should still exist
        assert!(repo.find_valid(&token_valid).await.unwrap().is_some());

        // Expired session should be cleaned up
        {
            let conn = pool.get().unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sessions WHERE token = ?",
                    [&token_expired],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
        }
    }
}
