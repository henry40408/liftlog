use chrono::{DateTime, Utc};
use rusqlite::Row;
use serde::{Deserialize, Serialize};

use super::FromSqliteRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    #[default]
    User,
}

impl UserRole {
    pub fn as_str(self) -> &'static str {
        match self {
            UserRole::Admin => "admin",
            UserRole::User => "user",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "admin" => UserRole::Admin,
            "user" => UserRole::User,
            other => {
                tracing::warn!(
                    role = other,
                    "unknown user role in DB; defaulting to UserRole::User",
                );
                UserRole::User
            }
        }
    }

    pub fn is_admin(self) -> bool {
        matches!(self, UserRole::Admin)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
}

impl FromSqliteRow for User {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let role_str: String = row.get("role")?;
        Ok(Self {
            id: row.get("id")?,
            username: row.get("username")?,
            password_hash: row.get("password_hash")?,
            role: UserRole::parse(&role_str),
            created_at: row.get("created_at")?,
        })
    }
}

/// Row shape for the admin users list. Deliberately omits `password_hash` —
/// the list never renders it, and pulling every user's Argon2 hash into a
/// template context is exposure with no upside.
#[derive(Debug, Clone)]
pub struct UserListItem {
    pub id: String,
    pub username: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
}

impl FromSqliteRow for UserListItem {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let role_str: String = row.get("role")?;
        Ok(Self {
            id: row.get("id")?,
            username: row.get("username")?,
            role: UserRole::parse(&role_str),
            created_at: row.get("created_at")?,
        })
    }
}

/// Minimum accepted password length, per the OWASP Authentication Cheat
/// Sheet's *Implement Proper Password Strength Controls*.
///
/// Enforced server-side in `validate_credentials` (signup, admin-created
/// users) and in the settings password-change handler; the `minlength` on the
/// signup forms only saves a round trip and is not the control. Changing this
/// constant does not invalidate passwords already stored — existing users keep
/// working until they next set one.
pub const MIN_PASSWORD_LEN: usize = 8;

#[derive(Debug, Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginCredentials {
    pub username: String,
    pub password: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_role_as_str() {
        assert_eq!(UserRole::Admin.as_str(), "admin");
        assert_eq!(UserRole::User.as_str(), "user");
    }

    #[test]
    fn test_user_role_parse() {
        assert_eq!(UserRole::parse("admin"), UserRole::Admin);
        assert_eq!(UserRole::parse("user"), UserRole::User);
        assert_eq!(UserRole::parse("unknown"), UserRole::User);
        assert_eq!(UserRole::parse(""), UserRole::User);
    }

    #[test]
    fn test_user_role_is_admin() {
        assert!(UserRole::Admin.is_admin());
        assert!(!UserRole::User.is_admin());
    }

    #[test]
    fn test_user_role_default() {
        let default_role: UserRole = UserRole::default();
        assert_eq!(default_role, UserRole::User);
    }
}
