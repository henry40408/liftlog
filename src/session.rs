use chrono::{DateTime, Utc};

use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::Cookie;

const SESSION_COOKIE_NAME_PLAIN: &str = "session";
const SESSION_COOKIE_NAME_HOST: &str = "__Host-session";

pub const SESSION_IDLE_TTL_SECS: i64 = 60 * 60 * 24 * 7; // 7 days

/// At most one `last_touched_at` write per session per window.
pub const SESSION_TOUCH_THROTTLE_SECS: i64 = 60 * 60; // 1 hour

/// Lifetime ceiling from `created_at`, not extendable by activity. Far above
/// OWASP's 4–8h on purpose: a gym journal that logs out mid-workout gets
/// worked around; what matters is that a ceiling exists.
pub const SESSION_ABSOLUTE_TTL_SECS: i64 = 60 * 60 * 24 * 90; // 90 days

/// Saturates rather than panicking on a corrupt `created_at` from the DB.
pub fn absolute_cap(created_at: DateTime<Utc>) -> DateTime<Utc> {
    created_at
        .checked_add_signed(chrono::Duration::seconds(SESSION_ABSOLUTE_TTL_SECS))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

/// What a touch on a session should write back to its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchAction {
    /// Inside the touch-throttle window: write nothing.
    Nothing,
    /// Outside the throttle, but `expires_at` is already pinned to the
    /// absolute cap: record the activity without extending the lifetime.
    TouchOnly,
    /// Outside the throttle and below the cap: slide `expires_at` to this
    /// instant, which is always `<= absolute_cap(created_at)`.
    Slide(DateTime<Utc>),
}

/// The throttle is checked first, even for a session pinned at the cap, or
/// pinned sessions would write on every request. A pinned session still gets
/// [`TouchAction::TouchOnly`] so `/settings` shows its real "last active".
pub fn compute_touch_action(
    created_at: DateTime<Utc>,
    last_touched_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> TouchAction {
    let throttle = chrono::Duration::seconds(SESSION_TOUCH_THROTTLE_SECS);
    if now - last_touched_at <= throttle {
        return TouchAction::Nothing;
    }

    let cap = absolute_cap(created_at);
    if expires_at >= cap {
        return TouchAction::TouchOnly;
    }

    let idle_ttl = chrono::Duration::seconds(SESSION_IDLE_TTL_SECS);
    TouchAction::Slide((now + idle_ttl).min(cap))
}

/// `__Host-` only when `secure`: over plain HTTP the browser silently drops a
/// `__Host-` cookie, and nobody can log in.
pub fn session_cookie_name(secure: bool) -> &'static str {
    if secure {
        SESSION_COOKIE_NAME_HOST
    } else {
        SESSION_COOKIE_NAME_PLAIN
    }
}

pub fn create_session_cookie(token: &str, secure: bool) -> Cookie<'static> {
    Cookie::build((session_cookie_name(secure), token.to_string()))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::seconds(SESSION_IDLE_TTL_SECS))
        .build()
}

/// No fallback to the other name: accepting a bare `session` under `secure`
/// would let a sibling subdomain inject one. Flipping the setting logs
/// everyone out once.
pub fn get_session_token(jar: &CookieJar, secure: bool) -> Option<String> {
    jar.get(session_cookie_name(secure))
        .map(|cookie| cookie.value().to_string())
}

/// Salted SHA-256, truncated to 16 hex chars, so logs can correlate a
/// session without containing its token.
pub fn token_fingerprint(token: &str, salt: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;

    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(token.as_bytes());
    let digest = hasher.finalize();

    let mut out = String::with_capacity(16);
    for byte in &digest[..8] {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Attributes must match [`create_session_cookie`]: a `__Host-` removal
/// lacking `Secure`/`Path=/`, or a `Secure` one over HTTP, is discarded and
/// the cookie survives logout.
pub fn remove_session_cookie(secure: bool) -> Cookie<'static> {
    Cookie::build((session_cookie_name(secure), ""))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::ZERO)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_session_cookie_sets_secure_when_enabled() {
        let cookie = create_session_cookie("tok", true);
        assert_eq!(cookie.secure(), Some(true));
    }

    #[test]
    fn create_session_cookie_omits_secure_when_disabled() {
        assert!(
            !create_session_cookie("tok", false)
                .to_string()
                .contains("Secure")
        );
    }

    #[test]
    fn remove_session_cookie_matches_create_attributes() {
        for secure in [true, false] {
            let created = create_session_cookie("tok", secure);
            let removed = remove_session_cookie(secure);

            assert_eq!(removed.secure(), created.secure());
            assert_eq!(removed.http_only(), created.http_only());
            assert_eq!(removed.path(), created.path());
            assert_eq!(removed.same_site(), created.same_site());
            assert_eq!(removed.max_age(), Some(time::Duration::ZERO));
        }
    }

    #[test]
    fn session_cookie_name_uses_host_prefix_only_when_secure() {
        assert_eq!(session_cookie_name(true), "__Host-session");
        assert_eq!(session_cookie_name(false), "session");
    }

    #[test]
    fn create_session_cookie_secure_satisfies_host_prefix_requirements() {
        let cookie = create_session_cookie("tok", true);
        assert!(cookie.name().starts_with("__Host-"));
        assert_eq!(cookie.secure(), Some(true));
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.domain(), None);
    }

    #[test]
    fn create_session_cookie_plain_has_no_host_prefix() {
        let cookie = create_session_cookie("tok", false);
        assert!(!cookie.name().starts_with("__Host-"));
        assert_eq!(cookie.name(), "session");
    }

    #[test]
    fn absolute_cap_is_created_at_plus_90_days() {
        let created_at = Utc::now();
        let cap = absolute_cap(created_at);
        assert_eq!(cap, created_at + chrono::Duration::days(90));
    }

    #[test]
    fn compute_touch_action_nothing_inside_throttle_window() {
        let now = Utc::now();
        let created_at = now - chrono::Duration::days(1);
        let last_touched_at = now - chrono::Duration::minutes(5);
        let expires_at = now + chrono::Duration::days(6);

        assert_eq!(
            compute_touch_action(created_at, last_touched_at, expires_at, now),
            TouchAction::Nothing
        );
    }

    #[test]
    fn compute_touch_action_slides_full_idle_ttl_for_young_session() {
        let now = Utc::now();
        let created_at = now - chrono::Duration::days(1);
        let last_touched_at = now - chrono::Duration::hours(2);
        let expires_at = now + chrono::Duration::days(6);

        assert_eq!(
            compute_touch_action(created_at, last_touched_at, expires_at, now),
            TouchAction::Slide(now + chrono::Duration::seconds(SESSION_IDLE_TTL_SECS))
        );
    }

    #[test]
    fn compute_touch_action_clamps_to_cap_near_the_limit() {
        let now = Utc::now();
        let created_at = now - chrono::Duration::days(89);
        let last_touched_at = now - chrono::Duration::hours(2);
        // Below the cap (now + 1d), so this hits the clamp, not TouchOnly.
        let expires_at = now + chrono::Duration::hours(3);

        let action = compute_touch_action(created_at, last_touched_at, expires_at, now);
        assert_eq!(action, TouchAction::Slide(absolute_cap(created_at)));
        assert_ne!(
            action,
            TouchAction::Slide(now + chrono::Duration::seconds(SESSION_IDLE_TTL_SECS))
        );
    }

    #[test]
    fn compute_touch_action_touch_only_when_pinned_at_cap() {
        let now = Utc::now();
        let created_at = now - chrono::Duration::days(89);
        let last_touched_at = now - chrono::Duration::hours(2);
        let expires_at = absolute_cap(created_at);

        assert_eq!(
            compute_touch_action(created_at, last_touched_at, expires_at, now),
            TouchAction::TouchOnly
        );
    }

    #[test]
    fn compute_touch_action_nothing_inside_throttle_even_when_pinned_at_cap() {
        let now = Utc::now();
        let created_at = now - chrono::Duration::days(89);
        let last_touched_at = now - chrono::Duration::minutes(5);
        let expires_at = absolute_cap(created_at);

        assert_eq!(
            compute_touch_action(created_at, last_touched_at, expires_at, now),
            TouchAction::Nothing
        );
    }

    #[test]
    fn absolute_cap_saturates_instead_of_panicking_near_datetime_max() {
        let created_at = DateTime::<Utc>::MAX_UTC - chrono::Duration::days(1);
        let cap = absolute_cap(created_at);
        assert_eq!(cap, DateTime::<Utc>::MAX_UTC);
    }

    #[test]
    fn token_fingerprint_is_deterministic_for_same_salt_and_token() {
        let token = "b6b1c1f4-6e1a-4e2a-9c2d-7f1a6d2e3b4c";
        let salt = [1u8; 32];
        assert_eq!(
            token_fingerprint(token, &salt),
            token_fingerprint(token, &salt)
        );
    }

    #[test]
    fn token_fingerprint_differs_for_different_salt() {
        let token = "b6b1c1f4-6e1a-4e2a-9c2d-7f1a6d2e3b4c";
        let salt_a = [1u8; 32];
        let salt_b = [2u8; 32];
        assert_ne!(
            token_fingerprint(token, &salt_a),
            token_fingerprint(token, &salt_b)
        );
    }

    #[test]
    fn token_fingerprint_differs_for_different_token() {
        let salt = [1u8; 32];
        let fp_a = token_fingerprint("b6b1c1f4-6e1a-4e2a-9c2d-7f1a6d2e3b4c", &salt);
        let fp_b = token_fingerprint("a1a1a1a1-1111-2222-3333-444455556666", &salt);
        assert_ne!(fp_a, fp_b);
    }

    #[test]
    fn token_fingerprint_never_contains_the_raw_token() {
        let token = "b6b1c1f4-6e1a-4e2a-9c2d-7f1a6d2e3b4c";
        let salt = [7u8; 32];
        let fp = token_fingerprint(token, &salt);

        assert!(!fp.contains(token));
        assert_eq!(fp.len(), 16);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
