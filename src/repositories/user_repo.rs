use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use chrono::Utc;
use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, Result};
use crate::models::{FromSqliteRow, User, UserListItem, UserRole};

#[derive(Clone)]
pub struct UserRepository {
    pool: DbPool,
}

impl UserRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn count(&self) -> Result<i64> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
            Ok(count)
        })
        .await?
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<User>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare("SELECT * FROM users WHERE id = ?")?;
            let result = stmt.query_row([&id], User::from_row).optional()?;
            Ok(result)
        })
        .await?
    }

    pub async fn find_by_username(&self, username: &str) -> Result<Option<User>> {
        let pool = self.pool.clone();
        let username = username.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare("SELECT * FROM users WHERE username = ?")?;
            let result = stmt.query_row([&username], User::from_row).optional()?;
            Ok(result)
        })
        .await?
    }

    /// Columns are listed explicitly rather than `SELECT *` so `password_hash`
    /// never leaves the DB for a read that only feeds the users list.
    pub async fn find_all(&self) -> Result<Vec<UserListItem>> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, username, role, created_at FROM users ORDER BY created_at DESC",
            )?;
            let users = stmt
                .query_map([], UserListItem::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(users)
        })
        .await?
    }

    pub async fn create(&self, username: &str, password: &str, role: UserRole) -> Result<User> {
        let pool = self.pool.clone();
        let username = username.to_string();
        let password = password.to_string();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        tokio::task::spawn_blocking(move || {
            let password_hash = hash_password(&password)?;
            let user = User {
                id,
                username,
                password_hash,
                role,
                created_at: now,
            };

            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?, ?, ?, ?, ?)",
                rusqlite::params![
                    user.id,
                    user.username,
                    user.password_hash,
                    user.role.as_str(),
                    user.created_at
                ],
            )?;
            Ok(user)
        })
        .await?
    }

    pub async fn change_password(&self, user_id: &str, new_password: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let new_password = new_password.to_string();

        tokio::task::spawn_blocking(move || {
            let password_hash = hash_password(&new_password)?;
            let conn = pool.get()?;
            let rows = conn.execute(
                "UPDATE users SET password_hash = ? WHERE id = ?",
                rusqlite::params![password_hash, user_id],
            )?;
            Ok(rows > 0)
        })
        .await?
    }

    /// Looks the user up and verifies the password inside a single blocking
    /// task, rather than composing `find_by_username` with a verify on the
    /// caller's thread.
    pub async fn verify_password(&self, username: &str, password: &str) -> Result<Option<User>> {
        let pool = self.pool.clone();
        let username = username.to_string();
        let password = password.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare("SELECT * FROM users WHERE username = ?")?;
            let user = stmt.query_row([&username], User::from_row).optional()?;

            let Some(user) = user else {
                // Spend the same Argon2 verification an existing username would
                // have cost. Without it the two paths differ by the whole cost
                // of a hash — tens of milliseconds against a bare SQLite lookup
                // — which is measurable from a browser without any statistical
                // work, and turns "is this a real account?" into a single
                // request. The generic "Invalid username or password" message
                // alone does not close that: the timing says what the wording
                // refuses to.
                //
                // The result is deliberately discarded; it can only be
                // `Ok(false)` (the supplied password will not match a hash of
                // DUMMY_PASSWORD) or a parse error that must not distinguish
                // this path from the other one either.
                let _ = verify_password(&password, dummy_password_hash());
                return Ok(None);
            };

            if verify_password(&password, &user.password_hash)? {
                Ok(Some(user))
            } else {
                Ok(None)
            }
        })
        .await?
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let rows = conn.execute("DELETE FROM users WHERE id = ?", [&id])?;
            Ok(rows > 0)
        })
        .await?
    }

    pub async fn update_role(&self, id: &str, role: UserRole) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let rows = conn.execute(
                "UPDATE users SET role = ? WHERE id = ?",
                rusqlite::params![role.as_str(), id],
            )?;
            Ok(rows > 0)
        })
        .await?
    }
}

/// Arbitrary; it is never a real credential. Only the hash derived from it is
/// used, and only to burn Argon2 time on the unknown-username login path.
const DUMMY_PASSWORD: &str = "liftlog-unknown-user-placeholder";

/// A valid Argon2 hash of [`DUMMY_PASSWORD`], for `verify_password`'s
/// unknown-username branch.
///
/// Computed once at first use rather than embedded as a literal, so it always
/// carries whatever parameters `Argon2::default()` currently produces. A
/// hardcoded PHC string would silently stop matching the real work — and
/// reopen the timing gap — the day that default changes.
fn dummy_password_hash() -> &'static str {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| hash_password(DUMMY_PASSWORD).expect("hashing a fixed literal cannot fail"))
}

/// Both of these run `Argon2::default()` — m=19 MiB, t=2, p=1, the parameters
/// OWASP recommends — which is tens of milliseconds of CPU per call, not a
/// negligible cost. Every caller in this module therefore invokes them from
/// inside `spawn_blocking`: on a tokio worker thread they would each pin a
/// core for that long, and `POST /settings/password` (two Argon2 operations
/// per request, and no rate limit, unlike login) is reachable by any
/// authenticated user in a loop.
fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_e| AppError::PasswordHash)?
        .to_string();
    Ok(password_hash)
}

fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let parsed_hash = PasswordHash::new(hash).map_err(|_e| AppError::PasswordHash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
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

    #[tokio::test]
    async fn test_count_empty_db() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let count = repo.count().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_create_user() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let user = repo
            .create("testuser", "password123", UserRole::User)
            .await
            .unwrap();

        assert_eq!(user.username, "testuser");
        assert_eq!(user.role, UserRole::User);
        assert!(!user.id.is_empty());
    }

    #[tokio::test]
    async fn test_create_user_admin_role() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let user = repo
            .create("admin", "adminpass", UserRole::Admin)
            .await
            .unwrap();

        assert_eq!(user.username, "admin");
        assert_eq!(user.role, UserRole::Admin);
    }

    #[tokio::test]
    async fn test_find_by_id_exists() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let created = repo
            .create("testuser", "password", UserRole::User)
            .await
            .unwrap();
        let found = repo.find_by_id(&created.id).await.unwrap();

        assert!(found.is_some());
        let user = found.unwrap();
        assert_eq!(user.id, created.id);
        assert_eq!(user.username, "testuser");
    }

    #[tokio::test]
    async fn test_find_by_id_not_exists() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let found = repo.find_by_id("nonexistent-id").await.unwrap();

        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_by_username_exists() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        repo.create("findme", "password", UserRole::User)
            .await
            .unwrap();
        let found = repo.find_by_username("findme").await.unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().username, "findme");
    }

    #[tokio::test]
    async fn test_find_by_username_not_exists() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let found = repo.find_by_username("nouser").await.unwrap();

        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_all_multiple() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        repo.create("user1", "pass1", UserRole::User).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        repo.create("user2", "pass2", UserRole::Admin)
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        repo.create("user3", "pass3", UserRole::User).await.unwrap();

        let users = repo.find_all().await.unwrap();

        assert_eq!(users.len(), 3);
        // Should be ordered by created_at DESC
        assert_eq!(users[0].username, "user3");
        assert_eq!(users[1].username, "user2");
        assert_eq!(users[2].username, "user1");
    }

    #[tokio::test]
    async fn test_verify_password_correct() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        repo.create("verifyuser", "correctpass", UserRole::User)
            .await
            .unwrap();
        let result = repo
            .verify_password("verifyuser", "correctpass")
            .await
            .unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap().username, "verifyuser");
    }

    #[tokio::test]
    async fn test_verify_password_incorrect() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        repo.create("verifyuser2", "correctpass", UserRole::User)
            .await
            .unwrap();
        let result = repo
            .verify_password("verifyuser2", "wrongpass")
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_verify_password_user_not_exists() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let result = repo.verify_password("nouser", "anypass").await.unwrap();

        assert!(result.is_none());
    }

    /// The timing defence only holds while the dummy hash costs the same as a
    /// real one, so pin the algorithm and cost parameters against a hash this
    /// codebase actually stores. Asserting on elapsed time instead would be
    /// flaky under a loaded CI runner; this catches the failure that matters —
    /// the dummy drifting away from `Argon2::default()`.
    #[tokio::test]
    async fn dummy_password_hash_matches_a_real_hash_parameter_for_parameter() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let real = repo
            .create("timinguser", "somepassword", UserRole::User)
            .await
            .unwrap()
            .password_hash;

        let real = PasswordHash::new(&real).unwrap();
        let dummy = PasswordHash::new(dummy_password_hash()).unwrap();

        assert_eq!(dummy.algorithm, real.algorithm);
        assert_eq!(dummy.params, real.params);
    }

    /// The salt must be per-hash, so the dummy is not a fixed string that
    /// could be recognised in a database dump or compared across deployments.
    #[test]
    fn dummy_password_hash_carries_its_own_salt() {
        let dummy = PasswordHash::new(dummy_password_hash()).unwrap();
        assert!(dummy.salt.is_some());
    }

    #[tokio::test]
    async fn test_delete_user_exists() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let user = repo
            .create("deleteuser", "pass", UserRole::User)
            .await
            .unwrap();
        let deleted = repo.delete(&user.id).await.unwrap();

        assert!(deleted);
        let found = repo.find_by_id(&user.id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_delete_user_not_exists() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let deleted = repo.delete("nonexistent-id").await.unwrap();

        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_update_role_success() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let user = repo
            .create("roleuser", "pass", UserRole::User)
            .await
            .unwrap();
        assert_eq!(user.role, UserRole::User);

        let updated = repo.update_role(&user.id, UserRole::Admin).await.unwrap();
        assert!(updated);

        let found = repo.find_by_id(&user.id).await.unwrap().unwrap();
        assert_eq!(found.role, UserRole::Admin);
    }

    #[tokio::test]
    async fn test_update_role_not_exists() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let updated = repo
            .update_role("nonexistent", UserRole::Admin)
            .await
            .unwrap();

        assert!(!updated);
    }

    #[tokio::test]
    async fn test_change_password_success() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let user = repo
            .create("pwuser", "oldpass123", UserRole::User)
            .await
            .unwrap();

        let changed = repo.change_password(&user.id, "newpass456").await.unwrap();
        assert!(changed);

        // Old password should no longer work
        let result = repo.verify_password("pwuser", "oldpass123").await.unwrap();
        assert!(result.is_none());

        // New password should work
        let result = repo.verify_password("pwuser", "newpass456").await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_change_password_not_exists() {
        let pool = setup_test_db();
        let repo = UserRepository::new(pool);

        let changed = repo
            .change_password("nonexistent", "newpass")
            .await
            .unwrap();
        assert!(!changed);
    }
}
