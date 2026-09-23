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

/// Admin users list row; omits `password_hash` on purpose.
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

/// 12 rather than NIST's 15-without-MFA: the `zxcvbn` floor does the
/// blocklist work, and together they reject more than 15 alone would
/// (`123456789012345` is 15 chars). Enforced server-side; the form's
/// `minlength` is only a convenience.
pub const MIN_PASSWORD_LEN: usize = 12;

/// Bounds Argon2 input against denial of service while fitting passphrases
/// (OWASP: ≥64).
/// Over-long passwords are rejected, never truncated.
pub const MAX_PASSWORD_LEN: usize = 128;

/// zxcvbn score (0–4). 3 is "safely unguessable" against an offline attack
/// on a slow hash; 4 mostly turns away real users.
const MIN_PASSWORD_SCORE: u8 = 3;

/// The user-facing rejection message, or `None` if acceptable. `label` names
/// the field, since the settings form has three password inputs.
///
/// 1. Length, in **characters** (not bytes, so non-Latin scripts aren't
///    penalised). Checked first so over-long input never reaches `zxcvbn`.
/// 2. `zxcvbn` score ≥ [`MIN_PASSWORD_SCORE`] — blocks common passwords, not
///    breached ones (no breach lookup; see README *Out of scope*).
///
/// `user_inputs` (the username) are penalised as attacker-known. Only
/// zxcvbn's prose feedback is shown, never its guess or crack-time estimates.
pub fn password_policy_error(password: &str, label: &str, user_inputs: &[&str]) -> Option<String> {
    let len = password.chars().count();
    if len < MIN_PASSWORD_LEN {
        return Some(format!(
            "{label} must be at least {MIN_PASSWORD_LEN} characters"
        ));
    }
    if len > MAX_PASSWORD_LEN {
        return Some(format!(
            "{label} must be at most {MAX_PASSWORD_LEN} characters"
        ));
    }

    let entropy = zxcvbn::zxcvbn(password, user_inputs);
    if u8::from(entropy.score()) >= MIN_PASSWORD_SCORE {
        return None;
    }

    // Prefer zxcvbn's own diagnosis; the generic fallback is unreachable in
    // practice (feedback is only absent for passing passwords).
    let feedback = entropy.feedback();
    let warning = feedback
        .and_then(zxcvbn::feedback::Feedback::warning)
        .map_or_else(
            || format!("{label} is too easy to guess."),
            |w| w.to_string(),
        );
    let suggestion = feedback
        .and_then(|f| f.suggestions().first())
        .map(|s| format!(" {s}"))
        .unwrap_or_default();
    Some(format!("{warning}{suggestion}"))
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

    /// Clears the score gate, so the other gate is what fires.
    const STRONG: &str = "deadlift squats bench";

    #[test]
    fn password_policy_rejects_too_short() {
        let message = password_policy_error(&"a".repeat(MIN_PASSWORD_LEN - 1), "Password", &[])
            .expect("a password below the minimum must be rejected");
        assert!(message.contains("at least 12 characters"), "got: {message}");
    }

    #[test]
    fn password_policy_rejects_too_long() {
        let long = STRONG.repeat(20);
        assert!(long.chars().count() > MAX_PASSWORD_LEN, "precondition");
        let message = password_policy_error(&long, "Password", &[])
            .expect("a password above the maximum must be rejected");
        assert!(message.contains("at most 128 characters"), "got: {message}");
    }

    #[test]
    fn password_policy_reports_length_before_strength() {
        let long_and_weak = "a".repeat(MAX_PASSWORD_LEN + 1);
        let message = password_policy_error(&long_and_weak, "Password", &[]).unwrap();
        assert!(
            message.contains("at most 128 characters"),
            "the length ceiling should be reported, not the strength verdict: {message}"
        );
    }

    #[test]
    fn password_policy_accepts_a_strong_password() {
        assert_eq!(password_policy_error(STRONG, "Password", &[]), None);
    }

    #[test]
    fn password_policy_accepts_the_minimum_length_when_strong() {
        let at_minimum = "gymrat.2026!";
        assert_eq!(at_minimum.chars().count(), MIN_PASSWORD_LEN, "precondition");
        assert_eq!(password_policy_error(at_minimum, "Password", &[]), None);
    }

    /// Passes any composition rule, still trivially guessable.
    #[test]
    fn password_policy_rejects_a_long_enough_but_guessable_password() {
        let message = password_policy_error("MyPassword12", "Password", &[])
            .expect("a common-pattern password must be rejected on strength");
        assert!(
            !message.contains("characters"),
            "should be a strength message, not a length one: {message}"
        );
        assert!(!message.is_empty());
    }

    #[test]
    fn password_policy_penalises_passwords_derived_from_the_username() {
        let derived = "henrylifts.42x";
        assert_eq!(
            password_policy_error(derived, "Password", &[]),
            None,
            "precondition: this clears the bar when the username is unknown"
        );
        assert!(
            password_policy_error(derived, "Password", &["henrylifts"]).is_some(),
            "the same password must be rejected once the username is supplied"
        );
    }

    /// Varied characters, not a repeat: zxcvbn would reject a repeat on
    /// strength and prove nothing about length.
    #[test]
    fn password_policy_counts_characters_not_bytes() {
        let twelve_cjk = "密碼鎖健身房訓練紀錄應用";
        assert_eq!(twelve_cjk.chars().count(), MIN_PASSWORD_LEN);
        assert!(
            twelve_cjk.len() > MIN_PASSWORD_LEN,
            "precondition: >12 bytes"
        );
        assert_eq!(password_policy_error(twelve_cjk, "Password", &[]), None);

        let eleven_cjk = "健身訓練紀錄鎖密碼安全";
        assert_eq!(eleven_cjk.chars().count(), MIN_PASSWORD_LEN - 1);
        assert!(
            eleven_cjk.len() > MIN_PASSWORD_LEN,
            "precondition: >12 bytes"
        );
        assert!(
            password_policy_error(eleven_cjk, "Password", &[])
                .unwrap()
                .contains("at least 12 characters"),
            "one character below the minimum must be rejected however many bytes it occupies"
        );
    }

    #[test]
    fn password_policy_uses_the_supplied_label_for_length_errors() {
        let message = password_policy_error("short", "New password", &[]).unwrap();
        assert!(message.starts_with("New password"), "got: {message}");
    }

    #[test]
    fn password_policy_surfaces_actionable_feedback() {
        let message = password_policy_error("password1234", "Password", &[]).unwrap();
        assert!(
            message.len() > 20,
            "expected zxcvbn's prose feedback, got: {message}"
        );
        for leaked in ["guesses", "score", "10^", "seconds"] {
            assert!(
                !message.to_lowercase().contains(leaked),
                "estimate detail leaked into the user-facing message: {message}"
            );
        }
    }
}
