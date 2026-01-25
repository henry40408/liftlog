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
