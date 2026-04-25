use chrono::Utc;
use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, Result};

#[derive(Clone)]
pub struct SessionRepository {
    pool: DbPool,
}

/// Returned by [`SessionRepository::validate_and_touch`].
pub struct ValidateAndTouchOutcome {
    pub user_id: String,
    /// `Some(new_expires)` iff this call wrote a new `last_touched_at` /
    /// `expires_at`. `None` means the call landed inside the throttle window.
    pub new_expires_at: Option<chrono::DateTime<Utc>>,
}

/// A single row returned by [`SessionRepository::list_for_user`].
pub struct SessionListRow {
    pub token: String,
    pub created_at: chrono::DateTime<Utc>,
    pub last_touched_at: chrono::DateTime<Utc>,
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
        let expires_at = now + chrono::Duration::seconds(crate::session::SESSION_IDLE_TTL_SECS);
        let token_clone = token.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO sessions (token, user_id, created_at, expires_at, last_touched_at) \
                 VALUES (?, ?, ?, ?, ?)",
                rusqlite::params![token_clone, user_id, now, expires_at, now],
            )?;
            Ok(token_clone)
        })
        .await?
    }

    /// Validate the session for a given token and, if the throttle window has
    /// elapsed, slide both `expires_at` and `last_touched_at` forward.
    /// Expired rows are lazily deleted.
    pub async fn validate_and_touch(&self, token: &str) -> Result<Option<ValidateAndTouchOutcome>> {
        let pool = self.pool.clone();
        let token = token.to_string();
        let now = Utc::now();
        let idle_ttl = chrono::Duration::seconds(crate::session::SESSION_IDLE_TTL_SECS);
        let throttle = chrono::Duration::seconds(crate::session::SESSION_TOUCH_THROTTLE_SECS);

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;

            let row: Option<(String, chrono::DateTime<Utc>, chrono::DateTime<Utc>)> = conn
                .query_row(
                    "SELECT user_id, expires_at, last_touched_at \
                     FROM sessions WHERE token = ?",
                    [&token],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;

            let Some((user_id, expires_at, last_touched_at)) = row else {
                return Ok::<_, AppError>(None);
            };

            if expires_at <= now {
                conn.execute("DELETE FROM sessions WHERE token = ?", [&token])?;
                return Ok(None);
            }

            if now - last_touched_at > throttle {
                let new_expires = now + idle_ttl;
                conn.execute(
                    "UPDATE sessions SET last_touched_at = ?, expires_at = ? WHERE token = ?",
                    rusqlite::params![now, new_expires, token],
                )?;
                return Ok(Some(ValidateAndTouchOutcome {
                    user_id,
                    new_expires_at: Some(new_expires),
                }));
            }

            Ok(Some(ValidateAndTouchOutcome {
                user_id,
                new_expires_at: None,
            }))
        })
        .await?
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
        .await?
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
        .await?
    }

    /// List all unexpired sessions for a user, newest-touched first.
    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<SessionListRow>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let now = Utc::now();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT token, created_at, last_touched_at FROM sessions \
                 WHERE user_id = ? AND expires_at > ? \
                 ORDER BY last_touched_at DESC",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![user_id, now], |row| {
                    Ok(SessionListRow {
                        token: row.get(0)?,
                        created_at: row.get(1)?,
                        last_touched_at: row.get(2)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await?
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
        .await?
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
    async fn test_create_and_validate_and_touch_within_window() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool);

        let token = repo.create(&user_id).await.unwrap();
        assert!(!token.is_empty());

        // Fresh session: last_touched_at is "now" so we are inside the throttle window.
        let outcome = repo.validate_and_touch(&token).await.unwrap().unwrap();
        assert_eq!(outcome.user_id, user_id);
        assert!(
            outcome.new_expires_at.is_none(),
            "touch should be absorbed by throttle window"
        );
    }

    #[tokio::test]
    async fn test_validate_and_touch_nonexistent() {
        let pool = setup_test_db();
        let repo = SessionRepository::new(pool);

        let found = repo.validate_and_touch("nonexistent-token").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_validate_and_touch_expired_deletes_row() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token = repo.create(&user_id).await.unwrap();

        // Move expires_at into the past.
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
                [&token],
            )
            .unwrap();
        }

        let outcome = repo.validate_and_touch(&token).await.unwrap();
        assert!(outcome.is_none());

        // Row is gone.
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
    async fn test_validate_and_touch_outside_window_slides_expiry() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token = repo.create(&user_id).await.unwrap();

        // Simulate an old session: last_touched_at 2 hours ago (> 1h throttle),
        // expires_at still in the future.
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET last_touched_at = datetime('now', '-2 hours'), \
                 expires_at = datetime('now', '+1 day') WHERE token = ?",
                [&token],
            )
            .unwrap();
        }

        let before_expires: chrono::DateTime<chrono::Utc> = {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT expires_at FROM sessions WHERE token = ?",
                [&token],
                |row| row.get(0),
            )
            .unwrap()
        };

        let outcome = repo.validate_and_touch(&token).await.unwrap().unwrap();
        assert_eq!(outcome.user_id, user_id);
        let new_expires = outcome
            .new_expires_at
            .expect("touch should advance expiry outside throttle window");
        assert!(new_expires > before_expires);

        // last_touched_at was refreshed.
        let conn = pool.get().unwrap();
        let last_touched: chrono::DateTime<chrono::Utc> = conn
            .query_row(
                "SELECT last_touched_at FROM sessions WHERE token = ?",
                [&token],
                |row| row.get(0),
            )
            .unwrap();
        let age = chrono::Utc::now() - last_touched;
        assert!(
            age.num_seconds().abs() < 5,
            "last_touched_at should be ~now"
        );
    }

    #[tokio::test]
    async fn test_delete() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool);

        let token = repo.create(&user_id).await.unwrap();
        repo.delete(&token).await.unwrap();

        let found = repo.validate_and_touch(&token).await.unwrap();
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

        repo.delete_all_for_user_except(&user_id, &token2)
            .await
            .unwrap();

        assert!(repo.validate_and_touch(&token1).await.unwrap().is_none());
        assert!(repo.validate_and_touch(&token2).await.unwrap().is_some());
        assert!(repo.validate_and_touch(&token3).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_list_for_user_returns_sessions_newest_first() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let t_old = repo.create(&user_id).await.unwrap();
        let t_mid = repo.create(&user_id).await.unwrap();
        let t_new = repo.create(&user_id).await.unwrap();

        // Stagger last_touched_at.
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET last_touched_at = datetime('now', '-3 days') WHERE token = ?",
                [&t_old],
            )
            .unwrap();
            conn.execute(
                "UPDATE sessions SET last_touched_at = datetime('now', '-1 day') WHERE token = ?",
                [&t_mid],
            )
            .unwrap();
        }

        let rows = repo.list_for_user(&user_id).await.unwrap();
        let tokens: Vec<_> = rows.iter().map(|r| r.token.as_str()).collect();
        assert_eq!(tokens, vec![t_new.as_str(), t_mid.as_str(), t_old.as_str()]);
    }

    #[tokio::test]
    async fn test_list_for_user_filters_expired() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let live = repo.create(&user_id).await.unwrap();
        let dead = repo.create(&user_id).await.unwrap();
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 minute') WHERE token = ?",
                [&dead],
            )
            .unwrap();
        }

        let rows = repo.list_for_user(&user_id).await.unwrap();
        let tokens: Vec<_> = rows.iter().map(|r| r.token.as_str()).collect();
        assert_eq!(tokens, vec![live.as_str()]);
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token_valid = repo.create(&user_id).await.unwrap();
        let token_expired = repo.create(&user_id).await.unwrap();

        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
                [&token_expired],
            )
            .unwrap();
        }

        repo.cleanup_expired().await.unwrap();

        assert!(repo
            .validate_and_touch(&token_valid)
            .await
            .unwrap()
            .is_some());

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
