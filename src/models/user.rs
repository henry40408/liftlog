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
///
/// NIST SP800-63B, which the cheat sheet cites, treats anything under 15
/// characters as weak when MFA is not available — and liftlog has no MFA. 15
/// is not used here because the strength check below is doing the work NIST's
/// blunt number stands in for: NIST's own advice is length **and blocklist
/// checks** over composition rules, and a length floor's job in that pairing
/// is to stop the trivially short, not to carry the whole policy alone. 12
/// with a `zxcvbn` floor rejects strictly more bad passwords than 15 alone
/// would (`123456789012345` is 15 characters), while staying short enough not
/// to push people toward writing it on a sticky note. Raise it here if you
/// want NIST's number literally.
pub const MIN_PASSWORD_LEN: usize = 12;

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

/// Lowest `zxcvbn` score a new password may have, on its 0–4 scale.
///
/// 3 is "safely unguessable" — zxcvbn's own bucket for 10^8–10^10 guesses,
/// i.e. resistant to an offline attack against a slow hash, which is what
/// liftlog stores (Argon2id at m=19 MiB). 4 would also reject things like
/// `benchpress123`, but the marginal password it turns away is far more often
/// a real user's than an attacker's guess, and a policy people fight is a
/// policy they work around. Raise it here if you want the stricter bar.
const MIN_PASSWORD_SCORE: u8 = 3;

/// Whether `password` is acceptable, returning the user-facing message when
/// it is not. `label` names the field in that message ("Password", "New
/// password") — the settings form has three password inputs, so a bare
/// "Password must be…" there would not say which one was wrong.
///
/// Two gates, in this order:
///
/// 1. Length must fall inside [`MIN_PASSWORD_LEN`]..=[`MAX_PASSWORD_LEN`].
///    Counts **characters, not bytes**. The cheat sheet requires that all
///    characters — "including unicode and whitespace" — be allowed, and a
///    byte count silently penalises them: under `str::len` a 4-character CJK
///    passphrase counts as 12, while an 80-character one counts as 240.
///    Counting chars makes the rule mean what the error message says it
///    means, in every script. The byte length is then bounded by
///    `4 * MAX_PASSWORD_LEN` (512), still a negligible input for Argon2, so
///    the denial-of-service ceiling holds regardless of encoding. Running
///    this gate **first** also bounds what reaches `zxcvbn`, whose cost grows
///    with input length.
///
/// 2. `zxcvbn` score must be at least [`MIN_PASSWORD_SCORE`]. This is the
///    cheat sheet's *Block common and previously breached passwords*
///    requirement: zxcvbn ships the common-password and English-word
///    dictionaries plus pattern detection (keyboard walks, l33t substitution,
///    dates, repeats), so it rejects `MyPassword12` — which every composition
///    rule ever written would happily accept.
///
/// `user_inputs` are strings zxcvbn should treat as known to an attacker and
/// penalise matches against; pass the username. Without it, `testuser1234` is
/// just an unremarkable mixed-case-and-digits string; with it, it is a
/// trivially guessable derivative of a public identifier.
///
/// Deliberately **not** surfaced to the user: zxcvbn's guess count and
/// crack-time estimates. The cheat sheet is explicit that a bits-of-entropy
/// figure must not be advertised as a guarantee of strength; the score gates
/// admission, and only zxcvbn's own prose feedback is shown.
///
/// Known limitation: zxcvbn's dictionaries are English-centric, so a
/// non-Latin-script password gets little signal from gate 2 and is protected
/// mostly by gate 1. That is a reason the length floor is not lowered on the
/// strength check's account.
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

    // Prefer zxcvbn's own diagnosis ("This is a top-10 common password") over
    // a generic rejection: a user told only "too weak" has no idea which of
    // the things they tried was the problem, and will usually just append a
    // digit. Feedback is absent only when the password passed, which this
    // branch has already ruled out — hence the fallback is unreachable in
    // practice but still has to say something useful.
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

    /// A strong password, used wherever a test needs the *other* gate to be
    /// the one that fires. Deliberately not a realistic user password — it
    /// only has to clear `MIN_PASSWORD_SCORE`.
    const STRONG: &str = "deadlift squats bench";

    #[test]
    fn password_policy_rejects_too_short() {
        let message = password_policy_error(&"a".repeat(MIN_PASSWORD_LEN - 1), "Password", &[])
            .expect("a password below the minimum must be rejected");
        assert!(message.contains("at least 12 characters"), "got: {message}");
    }

    #[test]
    fn password_policy_rejects_too_long() {
        // Strong enough to clear the score gate, so the length ceiling is
        // unambiguously what rejects it.
        let long = STRONG.repeat(20);
        assert!(long.chars().count() > MAX_PASSWORD_LEN, "precondition");
        let message = password_policy_error(&long, "Password", &[])
            .expect("a password above the maximum must be rejected");
        assert!(message.contains("at most 128 characters"), "got: {message}");
    }

    /// The length gate must run *before* the strength check, so an over-long
    /// input is never handed to zxcvbn — that ordering is what bounds the
    /// strength check's cost.
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

    /// The character-count boundary is inclusive; an off-by-one would move the
    /// real minimum to 13.
    #[test]
    fn password_policy_accepts_the_minimum_length_when_strong() {
        let at_minimum = "gymrat.2026!";
        assert_eq!(at_minimum.chars().count(), MIN_PASSWORD_LEN, "precondition");
        assert_eq!(password_policy_error(at_minimum, "Password", &[]), None);
    }

    /// The whole point of using zxcvbn rather than composition rules: this
    /// password has upper case, lower case and digits at 12 characters, so
    /// every rule-based policy ever written accepts it, and it is still
    /// trivially guessable.
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

    /// `user_inputs` is what makes a username-derived password guessable to
    /// the checker. Without it this string is unremarkable; with it, it is a
    /// derivative of a public identifier.
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

    /// The length gate counts characters, not bytes. Both strings below run
    /// well past 12 *bytes*, so a byte-based minimum would wave both through;
    /// they must be judged on their character counts instead. The 11-character
    /// one is the load-bearing half — zxcvbn has no CJK dictionary and scores
    /// it strong, so length is the only thing that can reject it.
    ///
    /// Note both are varied strings rather than one repeated character: a
    /// repeat is a pattern zxcvbn detects in any script, so `'鎖'.repeat(12)`
    /// would be rejected on *strength* and prove nothing about length.
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

    /// The settings form has three password inputs, so the message has to name
    /// the offending one.
    #[test]
    fn password_policy_uses_the_supplied_label_for_length_errors() {
        let message = password_policy_error("short", "New password", &[]).unwrap();
        assert!(message.starts_with("New password"), "got: {message}");
    }

    /// zxcvbn's own diagnosis is surfaced rather than a generic "too weak",
    /// so the user knows what to change. Its guess counts and crack-time
    /// estimates are deliberately never shown (the cheat sheet warns against
    /// advertising an entropy figure as a guarantee).
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
