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
