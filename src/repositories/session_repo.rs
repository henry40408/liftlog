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

/// The result of validating a session token, with enough detail for the
/// caller to emit a correct audit event. `Ok(None)` previously collapsed
/// "no such token" and "expired" into one case, which made it impossible
/// to distinguish a scanner probing random cookies from a real user whose
/// session aged out.
pub enum ValidateOutcome {
    /// Session is valid. Boxed because the payload is much larger than the
    /// other variants (`clippy::large_enum_variant`).
    Valid(Box<ValidateAndTouchOutcome>),
    /// `expires_at` had passed (idle timeout). The row has been deleted.
    ExpiredIdle,
    /// `created_at + SESSION_ABSOLUTE_TTL_SECS` had passed (absolute
    /// timeout). The row has been deleted.
    ExpiredAbsolute,
    /// The token is not in `sessions`, or its `user_id` no longer has a
    /// matching `users` row (the INNER JOIN missed).
    Unknown,
}

impl SessionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Returns the new session's token.
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
    pub async fn validate_and_touch(&self, token: &str) -> Result<ValidateOutcome> {
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
                return Ok::<_, AppError>(ValidateOutcome::Unknown);
            };
            let role = UserRole::parse(&role_str);

            if expires_at <= now {
                conn.execute("DELETE FROM sessions WHERE token = ?", [&token])?;
                return Ok(ValidateOutcome::ExpiredIdle);
            }

            // Checked second (not first) purely because `expires_at <= now` is
            // the common expiry path and this check needs an extra
            // computation; both branches delete-and-return, so the order
            // doesn't affect correctness.
            if now >= crate::session::absolute_cap(created_at) {
                conn.execute("DELETE FROM sessions WHERE token = ?", [&token])?;
                return Ok(ValidateOutcome::ExpiredAbsolute);
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
                    return Ok(ValidateOutcome::Valid(Box::new(ValidateAndTouchOutcome {
                        user_id,
                        username,
                        role,
                        new_expires_at: Some(new_expires),
                    })));
                }
            }

            // `TouchAction::Nothing` and `TouchAction::TouchOnly` both fall
            // through here: the session is still valid, it just isn't
            // sliding `expires_at` this time (inside the throttle window, or
            // already pinned to the absolute cap). Do NOT treat this as
            // invalid.
            Ok(ValidateOutcome::Valid(Box::new(ValidateAndTouchOutcome {
                user_id,
                username,
                role,
                new_expires_at: None,
            })))
        })
        .await?
    }

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

    /// Returns the number of rows deleted, for the audit log.
    pub async fn delete_all_for_user_except(
        &self,
        user_id: &str,
        keep_token: &str,
    ) -> Result<usize> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let keep_token = keep_token.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let count = conn.execute(
                "DELETE FROM sessions WHERE user_id = ? AND token != ?",
                rusqlite::params![user_id, keep_token],
            )?;
            Ok(count)
        })
        .await?
    }

    /// Delete every session for a user. Used when an admin deletes the
    /// account. `sessions.user_id` does have `ON DELETE CASCADE` and
    /// enforcement is on for every pooled connection, so in the normal case
    /// the cascade already removed these rows as part of the `users`
    /// delete and this is a no-op; it exists as a backstop in case a
    /// connection is ever handed out with `PRAGMA foreign_keys` off, since
    /// without it those rows would be orphaned rather than cleaned up.
    pub async fn delete_all_for_user(&self, user_id: &str) -> Result<usize> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let count = conn.execute("DELETE FROM sessions WHERE user_id = ?", [&user_id])?;
            Ok(count)
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

    /// Number of session rows a user currently has.
    ///
    /// Read before an admin deletes the account so the audit event can report
    /// how many sessions that action destroyed. Deriving the number from the
    /// subsequent `DELETE`'s row count instead would understate it whenever
    /// `SQLite`'s per-connection `foreign_keys` enforcement is active: the
    /// `ON DELETE CASCADE` on `sessions.user_id` removes the rows as part of
    /// the `users` delete, leaving the explicit cleanup nothing to count.
    pub async fn count_for_user(&self, user_id: &str) -> Result<usize> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sessions WHERE user_id = ?",
                [&user_id],
                |row| row.get(0),
            )?;
            // COUNT(*) can never be negative, so try_from can never actually
            // fail here; unwrap_or(0) just avoids an `as` cast (a sign-loss
            // cast under this crate's pedantic lint config) without a panic
            // path for a case that cannot occur.
            Ok(usize::try_from(count).unwrap_or(0))
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
    ///
    /// Returns the number of rows retired so the caller can log it.
    pub async fn cleanup_expired(&self) -> Result<usize> {
        let pool = self.pool.clone();
        let now = Utc::now();
        let oldest_allowed_creation =
            now - chrono::Duration::seconds(crate::session::SESSION_ABSOLUTE_TTL_SECS);

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let count = conn.execute(
                "DELETE FROM sessions WHERE expires_at <= ? OR created_at <= ?",
                rusqlite::params![now, oldest_allowed_creation],
            )?;
            Ok(count)
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
        let outcome = repo.validate_and_touch(&token).await.unwrap();
        let ValidateOutcome::Valid(outcome) = outcome else {
            panic!("expected Valid outcome");
        };
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
        assert!(matches!(found, ValidateOutcome::Unknown));
    }

    #[tokio::test]
    async fn test_validate_and_touch_expired_deletes_row() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token = repo.create(&user_id).await.unwrap();

        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
                [&token],
            )
            .unwrap();
        }

        let outcome = repo.validate_and_touch(&token).await.unwrap();
        assert!(matches!(outcome, ValidateOutcome::ExpiredIdle));

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

        let outcome = repo.validate_and_touch(&token).await.unwrap();
        let ValidateOutcome::Valid(outcome) = outcome else {
            panic!("expected Valid outcome");
        };
        assert_eq!(outcome.user_id, user_id);
        let new_expires = outcome
            .new_expires_at
            .expect("touch should advance expiry outside throttle window");
        assert!(new_expires > before_expires);

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
        assert!(matches!(outcome, ValidateOutcome::ExpiredAbsolute));

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

        let outcome = repo.validate_and_touch(&token).await.unwrap();
        let ValidateOutcome::Valid(outcome) = outcome else {
            panic!("expected Valid outcome");
        };
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
        let ValidateOutcome::Valid(outcome) = outcome else {
            panic!("session should still be valid");
        };
        assert!(
            outcome.new_expires_at.is_none(),
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
        assert!(matches!(found, ValidateOutcome::Unknown));
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

        assert!(matches!(
            repo.validate_and_touch(&token1).await.unwrap(),
            ValidateOutcome::Unknown
        ));
        assert!(matches!(
            repo.validate_and_touch(&token2).await.unwrap(),
            ValidateOutcome::Valid(_)
        ));
        assert!(matches!(
            repo.validate_and_touch(&token3).await.unwrap(),
            ValidateOutcome::Unknown
        ));
    }

    #[tokio::test]
    async fn test_delete_all_for_user_except_returns_deleted_count() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool);

        let _token1 = repo.create(&user_id).await.unwrap();
        let token2 = repo.create(&user_id).await.unwrap();
        let _token3 = repo.create(&user_id).await.unwrap();

        let count = repo
            .delete_all_for_user_except(&user_id, &token2)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_delete_all_for_user_returns_count_and_removes_every_row() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool);

        let token1 = repo.create(&user_id).await.unwrap();
        let token2 = repo.create(&user_id).await.unwrap();
        let token3 = repo.create(&user_id).await.unwrap();

        let count = repo.delete_all_for_user(&user_id).await.unwrap();
        assert_eq!(count, 3);

        for token in [&token1, &token2, &token3] {
            assert!(matches!(
                repo.validate_and_touch(token).await.unwrap(),
                ValidateOutcome::Unknown
            ));
        }
    }

    #[tokio::test]
    async fn test_count_for_user_counts_only_that_users_sessions() {
        let pool = setup_test_db();
        let user1 = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let user_repo = UserRepository::new(pool.clone());
        let user2 = user_repo
            .create("otheruser", "password", UserRole::User)
            .await
            .unwrap()
            .id;

        let _t1 = repo.create(&user1).await.unwrap();
        let _t2 = repo.create(&user1).await.unwrap();
        let _t3 = repo.create(&user2).await.unwrap();

        assert_eq!(repo.count_for_user(&user1).await.unwrap(), 2);
        assert_eq!(repo.count_for_user(&user2).await.unwrap(), 1);
        assert_eq!(repo.count_for_user("no-such-user").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_validate_and_touch_distinguishes_idle_from_absolute_expiry() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let idle_token = repo.create(&user_id).await.unwrap();
        let absolute_token = repo.create(&user_id).await.unwrap();

        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
                [&idle_token],
            )
            .unwrap();
            conn.execute(
                "UPDATE sessions SET created_at = datetime('now', '-91 days'), \
                 expires_at = datetime('now', '+1 day') WHERE token = ?",
                [&absolute_token],
            )
            .unwrap();
        }

        assert!(matches!(
            repo.validate_and_touch(&idle_token).await.unwrap(),
            ValidateOutcome::ExpiredIdle
        ));
        assert!(matches!(
            repo.validate_and_touch(&absolute_token).await.unwrap(),
            ValidateOutcome::ExpiredAbsolute
        ));

        let conn = pool.get().unwrap();
        for token in [&idle_token, &absolute_token] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sessions WHERE token = ?",
                    [token],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "row for {token} should be deleted");
        }
    }

    #[tokio::test]
    async fn test_validate_and_touch_unknown_when_user_row_is_gone() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let token = repo.create(&user_id).await.unwrap();

        {
            let conn = pool.get().unwrap();
            conn.execute("DELETE FROM users WHERE id = ?", [&user_id])
                .unwrap();
        }

        assert!(matches!(
            repo.validate_and_touch(&token).await.unwrap(),
            ValidateOutcome::Unknown
        ));
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

        assert!(matches!(
            repo.validate_and_touch(&token_valid).await.unwrap(),
            ValidateOutcome::Valid(_)
        ));

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

        assert!(matches!(
            repo.validate_and_touch(&token_valid).await.unwrap(),
            ValidateOutcome::Valid(_)
        ));

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

    #[tokio::test]
    async fn test_cleanup_expired_returns_the_number_of_rows_retired() {
        let pool = setup_test_db();
        let user_id = create_user(&pool).await;
        let repo = SessionRepository::new(pool.clone());

        let _token_valid = repo.create(&user_id).await.unwrap();
        let token_expired1 = repo.create(&user_id).await.unwrap();
        let token_expired2 = repo.create(&user_id).await.unwrap();

        {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
                [&token_expired1],
            )
            .unwrap();
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
                [&token_expired2],
            )
            .unwrap();
        }

        let count = repo.cleanup_expired().await.unwrap();
        assert_eq!(count, 2);

        let conn = pool.get().unwrap();
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 1);
    }
}
