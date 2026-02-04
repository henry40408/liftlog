use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::Utc;
use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, Result};
use crate::models::{FromSqliteRow, User, UserRole};

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
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    #[allow(dead_code)]
    pub async fn find_by_id(&self, id: &str) -> Result<Option<User>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare("SELECT * FROM users WHERE id = ?")?;
            let result = stmt.query_row([&id], User::from_row).optional()?;
            Ok(result)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
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
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn find_all(&self) -> Result<Vec<User>> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare("SELECT * FROM users ORDER BY created_at DESC")?;
            let users = stmt
                .query_map([], User::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(users)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn create(&self, username: &str, password: &str, role: UserRole) -> Result<User> {
        let password_hash = hash_password(password)?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let username = username.to_string();

        let pool = self.pool.clone();
        let user = User {
            id: id.clone(),
            username: username.clone(),
            password_hash,
            role,
            created_at: now,
        };
        let user_clone = user.clone();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?, ?, ?, ?, ?)",
                rusqlite::params![
                    user_clone.id,
                    user_clone.username,
                    user_clone.password_hash,
                    user_clone.role.as_str(),
                    user_clone.created_at
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;

        Ok(user)
    }

    pub async fn change_password(&self, user_id: &str, new_password: &str) -> Result<bool> {
        let password_hash = hash_password(new_password)?;
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let rows = conn.execute(
                "UPDATE users SET password_hash = ? WHERE id = ?",
                rusqlite::params![password_hash, user_id],
            )?;
            Ok(rows > 0)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    pub async fn verify_password(&self, username: &str, password: &str) -> Result<Option<User>> {
        let user = self.find_by_username(username).await?;

        match user {
            Some(user) => {
                if verify_password(password, &user.password_hash)? {
                    Ok(Some(user))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            let rows = conn.execute("DELETE FROM users WHERE id = ?", [&id])?;
            Ok(rows > 0)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
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
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }
}

fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| AppError::PasswordHash)?
        .to_string();
    Ok(password_hash)
}

fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let parsed_hash = PasswordHash::new(hash).map_err(|_| AppError::PasswordHash)?;
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
}
