use chrono::Utc;
use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, Result};
use crate::models::UserRole;

#[derive(Clone)]
pub struct SessionRepository {
    pool: DbPool,
}

/// Returned by [`SessionRepository::validate_and_touch`]. Carries the full
/// session+user identity so downstream extractors don't need a second
/// `users` lookup per request.
pub struct ValidateAndTouchOutcome {
    pub user_id: String,
    pub username: String,
    pub role: UserRole,
    /// `Some(new_expires)` iff this call slid `expires_at` forward.
    /// `None` means the call did not extend the lifetime — either it landed
    /// inside the throttle window, or `expires_at` is already pinned to
    /// `absolute_cap(created_at)`. In both cases the session is still
    /// valid; `None` here must never be read as "session invalid".
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

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;

            type Row = (
                String,
                chrono::DateTime<Utc>,
                chrono::DateTime<Utc>,
                chrono::DateTime<Utc>,
                String,
                String,
            );
            let row: Option<Row> = conn
                .query_row(
                    "SELECT s.user_id, s.created_at, s.expires_at, s.last_touched_at, u.username, u.role \
                     FROM sessions s JOIN users u ON u.id = s.user_id \
                     WHERE s.token = ?",
                    [&token],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )
                .optional()?;

            let Some((user_id, created_at, expires_at, last_touched_at, username, role_str)) = row
            else {
                return Ok::<_, AppError>(None);
            };
            let role = UserRole::parse(&role_str);

            if expires_at <= now {
                conn.execute("DELETE FROM sessions WHERE token = ?", [&token])?;
                return Ok(None);
            }

            // Checked second (not first) purely because `expires_at <= now` is
            // the common expiry path and this check needs an extra
            // computation; both branches delete-and-return, so the order
            // doesn't affect correctness.
            if now >= crate::session::absolute_cap(created_at) {
                conn.execute("DELETE FROM sessions WHERE token = ?", [&token])?;
                return Ok(None);
            }

            match crate::session::compute_touch_action(created_at, last_touched_at, expires_at, now)
            {
                crate::session::TouchAction::Nothing => {}
                crate::session::TouchAction::TouchOnly => {
                    conn.execute(
                        "UPDATE sessions SET last_touched_at = ? WHERE token = ?",
                        rusqlite::params![now, token],
                    )?;
                }
                crate::session::TouchAction::Slide(new_expires) => {
                    conn.execute(
                        "UPDATE sessions SET last_touched_at = ?, expires_at = ? WHERE token = ?",
                        rusqlite::params![now, new_expires, token],
                    )?;
                    return Ok(Some(ValidateAndTouchOutcome {
                        user_id,
                        username,
                        role,
                        new_expires_at: Some(new_expires),
                    }));
                }
            }

            // `TouchAction::Nothing` and `TouchAction::TouchOnly` both fall
            // through here: the session is still valid, it just isn't
            // sliding `expires_at` this time (inside the throttle window, or
            // already pinned to the absolute cap). Do NOT treat this as
            // invalid.
            Ok(Some(ValidateAndTouchOutcome {
                user_id,
                username,
                role,
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
    ///
    /// `expires_at` alone stays correct under the absolute-cap rule since
    /// `expires_at` is now always clamped to `created_at + 90d`. The extra
    /// `created_at` filter exists only to hide over-age rows that were
    /// written *before* this change shipped (their `expires_at` may still be
    /// unclamped and in the future) until the hourly sweep in
    /// `cleanup_expired` retires them.
    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<SessionListRow>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let now = Utc::now();
        let oldest_allowed_creation =
            now - chrono::Duration::seconds(crate::session::SESSION_ABSOLUTE_TTL_SECS);

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT token, created_at, last_touched_at FROM sessions \
                 WHERE user_id = ? AND expires_at > ? AND created_at > ? \
                 ORDER BY last_touched_at DESC",
            )?;
            let rows = stmt
                .query_map(
                    rusqlite::params![user_id, now, oldest_allowed_creation],
                    |row| {
                        Ok(SessionListRow {
                            token: row.get(0)?,
                            created_at: row.get(1)?,
                            last_touched_at: row.get(2)?,
                        })
                    },
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await?
    }

    /// Batch delete all expired sessions.
    ///
    /// The `created_at` arm exists because `validate_and_touch` slid
    /// `expires_at` unclamped before this change shipped: a session
    /// touched once a week could carry an `expires_at` far in the future
    /// even though it is now over the 90-day absolute cap. This lets the
    /// hourly background sweep (`sweep_handle` in `main.rs`) retire those
    /// legacy rows without a data migration.
    pub async fn cleanup_expired(&self) -> Result<()> {
        let pool = self.pool.clone();
        let now = Utc::now();
        let oldest_allowed_creation =
            now - chrono::Duration::seconds(crate::session::SESSION_ABSOLUTE_TTL_SECS);

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute(
                "DELETE FROM sessions WHERE expires_at <= ? OR created_at <= ?",
                rusqlite::params![now, oldest_allowed_creation],
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
    async fn test_validate_and_touch_deletes_session_past_absolute_cap() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token = repo.create(&user_id).await.unwrap();

        // Over the 90-day absolute cap, even though expires_at is still in
        // the future and last_touched_at is fresh. This is the core proof
        // that age alone, not activity, terminates the session.
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET created_at = datetime('now', '-91 days'), \
                 last_touched_at = datetime('now'), \
                 expires_at = datetime('now', '+1 day') WHERE token = ?",
                [&token],
            )
            .unwrap();
        }

        let outcome = repo.validate_and_touch(&token).await.unwrap();
        assert!(outcome.is_none());

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

    #[tokio::test]
    async fn test_validate_and_touch_clamps_expiry_to_absolute_cap() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token = repo.create(&user_id).await.unwrap();

        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET created_at = datetime('now', '-89 days'), \
                 last_touched_at = datetime('now', '-2 hours'), \
                 expires_at = datetime('now', '+3 hours') WHERE token = ?",
                [&token],
            )
            .unwrap();
        }

        let created_at: chrono::DateTime<chrono::Utc> = {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT created_at FROM sessions WHERE token = ?",
                [&token],
                |row| row.get(0),
            )
            .unwrap()
        };

        let outcome = repo.validate_and_touch(&token).await.unwrap().unwrap();
        let new_expires = outcome
            .new_expires_at
            .expect("touch should still slide when below the cap");
        let expected_cap = created_at + chrono::Duration::days(90);
        let drift = (new_expires - expected_cap).num_seconds().abs();
        assert!(
            drift < 5,
            "new_expires should be pinned to created_at + 90d, got {new_expires}, expected ~{expected_cap}"
        );
    }

    #[tokio::test]
    async fn test_validate_and_touch_stops_sliding_at_cap_but_session_still_valid() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token = repo.create(&user_id).await.unwrap();

        // expires_at already pinned to the cap; last_touched_at is outside
        // the throttle window, so the only reason to not slide is the cap.
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET created_at = datetime('now', '-1 day'), \
                 last_touched_at = datetime('now', '-2 hours'), \
                 expires_at = datetime('now', '+89 days') WHERE token = ?",
                [&token],
            )
            .unwrap();
        }

        let outcome = repo.validate_and_touch(&token).await.unwrap();
        assert!(outcome.is_some(), "session should still be valid");
        assert!(
            outcome.unwrap().new_expires_at.is_none(),
            "expiry should not slide past the absolute cap"
        );
    }

    #[tokio::test]
    async fn test_validate_and_touch_at_cap_still_records_activity() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token = repo.create(&user_id).await.unwrap();

        // expires_at already pinned to the cap, last_touched_at stale (>1h
        // throttle): this is the exact scenario that used to freeze
        // last_touched_at forever once a session hit the cap.
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET created_at = datetime('now', '-1 day'), \
                 last_touched_at = datetime('now', '-2 hours'), \
                 expires_at = datetime('now', '+89 days') WHERE token = ?",
                [&token],
            )
            .unwrap();
        }

        let expires_before: chrono::DateTime<chrono::Utc> = {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT expires_at FROM sessions WHERE token = ?",
                [&token],
                |row| row.get(0),
            )
            .unwrap()
        };

        let outcome = repo.validate_and_touch(&token).await.unwrap();
        assert!(outcome.is_some(), "session should still be valid");
        assert!(
            outcome.unwrap().new_expires_at.is_none(),
            "expiry should not slide past the absolute cap"
        );

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
            "last_touched_at should be recorded as ~now even when pinned at the cap"
        );

        let expires_after: chrono::DateTime<chrono::Utc> = conn
            .query_row(
                "SELECT expires_at FROM sessions WHERE token = ?",
                [&token],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            expires_after, expires_before,
            "expires_at must be unchanged when pinned at the cap"
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
    async fn test_list_for_user_filters_over_age_rows() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let live = repo.create(&user_id).await.unwrap();
        // Legacy row: over-age but expires_at was slid unclamped before this
        // change shipped, so it is still in the future.
        let over_age = repo.create(&user_id).await.unwrap();
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET created_at = datetime('now', '-91 days'), \
                 expires_at = datetime('now', '+1 day') WHERE token = ?",
                [&over_age],
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

        assert!(
            repo.validate_and_touch(&token_valid)
                .await
                .unwrap()
                .is_some()
        );

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

    #[tokio::test]
    async fn test_cleanup_expired_removes_over_age_rows() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token_valid = repo.create(&user_id).await.unwrap();
        // Legacy row: over the absolute cap but expires_at is still in the
        // future (as it could be for a session that was slid before this
        // change shipped).
        let token_over_age = repo.create(&user_id).await.unwrap();

        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET created_at = datetime('now', '-91 days'), \
                 expires_at = datetime('now', '+1 day') WHERE token = ?",
                [&token_over_age],
            )
            .unwrap();
        }

        repo.cleanup_expired().await.unwrap();

        assert!(
            repo.validate_and_touch(&token_valid)
                .await
                .unwrap()
                .is_some()
        );

        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE token = ?",
                [&token_over_age],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
