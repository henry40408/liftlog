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

/// Maximum accepted password length. The cheat sheet asks for two things that
/// pull in opposite directions, and this satisfies both: a maximum of *at
/// least* 64 so passphrases fit (*Implement Proper Password Strength
/// Controls*), and a maximum at all so the hash comparison has a bounded
/// input (*Compare Password Hashes Using Safe Functions*: "Has a maximum
/// input length, to protect against denial of service attacks with very long
/// inputs").
///
/// Without a ceiling, `axum`'s default 2 MiB body limit was the only bound on
/// what reached Argon2 — and `POST /settings/password` runs two Argon2
/// operations per request. Over-long passwords are **rejected, never
/// truncated**: silent truncation would let a user believe a long passphrase
/// protects them while only its prefix is ever checked.
pub const MAX_PASSWORD_LEN: usize = 128;

/// Whether `password` falls inside [`MIN_PASSWORD_LEN`]..=[`MAX_PASSWORD_LEN`],
/// returning the user-facing message when it does not. `label` names the
/// field in that message ("Password", "New password") — the settings form has
/// three password inputs, so a bare "Password must be…" there would not say
/// which one was wrong.
///
/// Counts **characters, not bytes**. The cheat sheet requires that all
/// characters — "including unicode and whitespace" — be allowed, and a byte
/// count silently penalises them: under `str::len` a 4-character CJK
/// passphrase counts as 12 and clears an 8-byte minimum, while an 80-character
/// one counts as 240. Counting chars makes the rule mean what the error
/// message says it means, in every script. The byte length is then bounded by
/// `4 * MAX_PASSWORD_LEN` (512), still a negligible input for Argon2, so the
/// denial-of-service ceiling above holds regardless of encoding.
pub fn password_length_error(password: &str, label: &str) -> Option<String> {
    let len = password.chars().count();
    if len < MIN_PASSWORD_LEN {
        Some(format!(
            "{label} must be at least {MIN_PASSWORD_LEN} characters"
        ))
    } else if len > MAX_PASSWORD_LEN {
        Some(format!(
            "{label} must be at most {MAX_PASSWORD_LEN} characters"
        ))
    } else {
        None
    }
}

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

    #[test]
    fn password_length_error_rejects_too_short() {
        let message = password_length_error(&"a".repeat(MIN_PASSWORD_LEN - 1), "Password")
            .expect("a password below the minimum must be rejected");
        assert!(message.contains("at least 8 characters"), "got: {message}");
    }

    #[test]
    fn password_length_error_rejects_too_long() {
        let message = password_length_error(&"a".repeat(MAX_PASSWORD_LEN + 1), "Password")
            .expect("a password above the maximum must be rejected");
        assert!(message.contains("at most 128 characters"), "got: {message}");
    }

    /// Both bounds are inclusive, so neither boundary value may be rejected —
    /// an off-by-one here would silently move the real minimum to 9 or the
    /// real maximum to 127.
    #[test]
    fn password_length_error_accepts_both_boundaries() {
        assert!(password_length_error(&"a".repeat(MIN_PASSWORD_LEN), "Password").is_none());
        assert!(password_length_error(&"a".repeat(MAX_PASSWORD_LEN), "Password").is_none());
    }

    /// The bound is on characters, not bytes. Under the old `str::len` check a
    /// 3-character CJK password measured 9 bytes and cleared the 8-character
    /// minimum; a 40-character CJK passphrase measured 120 bytes. Pin both
    /// directions so a revert to byte counting fails loudly.
    #[test]
    fn password_length_error_counts_characters_not_bytes() {
        let three_chars = "密碼鎖";
        assert_eq!(three_chars.len(), 9, "precondition: 9 bytes, 3 chars");
        assert!(
            password_length_error(three_chars, "Password").is_some(),
            "a 3-character password must be rejected however many bytes it occupies"
        );

        let long_but_legal: String = std::iter::repeat_n('鎖', MAX_PASSWORD_LEN).collect();
        assert!(long_but_legal.len() > MAX_PASSWORD_LEN, "precondition");
        assert!(
            password_length_error(&long_but_legal, "Password").is_none(),
            "a password at exactly the character limit must be accepted in any script"
        );
    }

    /// The settings form has three password inputs, so the message has to name
    /// the offending one.
    #[test]
    fn password_length_error_uses_the_supplied_label() {
        let message = password_length_error("short", "New password").unwrap();
        assert!(message.starts_with("New password"), "got: {message}");
    }
}
