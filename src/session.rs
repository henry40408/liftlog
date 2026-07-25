use chrono::{DateTime, Utc};

use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::Cookie;

const SESSION_COOKIE_NAME_PLAIN: &str = "session";
const SESSION_COOKIE_NAME_HOST: &str = "__Host-session";

/// How long a session survives without activity. A request within this
/// window (and outside the touch throttle) slides the expiry forward.
pub const SESSION_IDLE_TTL_SECS: i64 = 60 * 60 * 24 * 7; // 7 days

/// Minimum gap between two consecutive `last_touched_at` writes for the
/// same session. Keeps write load to at most one UPDATE per session per hour.
pub const SESSION_TOUCH_THROTTLE_SECS: i64 = 60 * 60; // 1 hour

/// Absolute ceiling on a session's lifetime, measured from `created_at` and
/// not extendable by activity — a session touched regularly can slide its
/// idle expiry forever, but it can never live past `created_at + this`.
///
/// This is deliberately far larger than the 4-8 hours OWASP suggests for
/// high-value applications. liftlog is a personal, self-hosted workout
/// journal; an 8-hour ceiling would log users out several times a day at the
/// gym, a cost far exceeding the benefit, and would push people toward
/// working around it. The requirement being satisfied here is that an
/// absolute ceiling exists at all — a session touched once a week can no
/// longer live forever. 90 days matches the sibling project `rdrs`
/// (`SESSION_ABSOLUTE_MAX_DAYS = 90`). It is a constant rather than an env
/// var to keep the change minimal.
pub const SESSION_ABSOLUTE_TTL_SECS: i64 = 60 * 60 * 24 * 90; // 90 days

/// The absolute expiry instant for a session created at `created_at`. Past
/// this instant the session is dead regardless of activity.
///
/// Uses `checked_add_signed` rather than plain `+` because `created_at` is
/// read straight out of `SQLite`: a corrupt or absurd row (e.g. a `created_at`
/// near `DateTime::<Utc>::MAX_UTC`) must not panic every request that
/// carries its session token. Saturates to `DateTime::<Utc>::MAX_UTC`
/// instead, which is safe: it just makes an already-degenerate session's cap
/// unreachably far away rather than crashing the request.
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

/// Decides what a touch on a session should do.
///
/// The throttle check runs first, unconditionally — including when the
/// session is already pinned at the absolute cap. Without that ordering, a
/// pinned session would run `UPDATE sessions SET last_touched_at = ?` on
/// every single request instead of at most once per throttle window, which
/// defeats the point of throttling.
///
/// Once outside the throttle window: a session already pinned to
/// `absolute_cap(created_at)` cannot slide any further, but the request
/// still happened, so [`TouchAction::TouchOnly`] records that activity
/// (`last_touched_at`) without touching `expires_at`. `/settings`'s session
/// list orders and displays by `last_touched_at`, so skipping this write
/// entirely — as the old `compute_touched_expiry` did — froze "last active"
/// at the moment a session hit the cap, for up to the final 7 days of its
/// 90-day lifetime, even while the user kept using it.
///
/// Below the cap, [`TouchAction::Slide`] carries the new `expires_at`.
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

/// Name to use for the session cookie. Switching on `secure` here is
/// **mandatory**, not cosmetic: applying the `__Host-` prefix unconditionally
/// on a plain-HTTP deployment makes the browser discard the entire
/// `Set-Cookie` line (the prefix requires `Secure`, and a `Secure` cookie
/// cannot arrive over HTTP), so users cannot log in at all and get no error
/// message. This is the easiest mistake to make here and the hardest to
/// diagnose.
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

/// Looks up the session token under exactly one cookie name:
/// `session_cookie_name(secure)`. Deliberately no fallback to the other
/// name — accepting a bare `session` while `secure = true` would forfeit
/// the whole point of `__Host-`, since an attacker able to write cookies on
/// a sibling subdomain could just inject the unprefixed name. The cost is
/// that flipping `COOKIE_SECURE` logs everyone out once, which is a one-off
/// and acceptable.
pub fn get_session_token(jar: &CookieJar, secure: bool) -> Option<String> {
    jar.get(session_cookie_name(secure))
        .map(|cookie| cookie.value().to_string())
}

/// Salted-hash fingerprint of a session token, for log correlation.
///
/// OWASP requires that the session ID never be written to a log; a salted
/// hash lets events for the same session be correlated without disclosing
/// the token. 16 hex chars (64 bits) is ample to avoid collisions at a
/// single deployment's session volume while keeping log lines readable.
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

/// Builds the cookie used to clear a session on logout. Its attributes
/// (`secure`, `http_only`, `same_site`, `path`) MUST match the create-side
/// cookie exactly, and not because a non-`Secure` removal cookie fails to
/// clear a `Secure` one — a browser's cookie identity key is
/// `(name, domain, path)`, and `Secure` is not part of that key, so that
/// intuition is a misconception. The real failure mode is that a mismatched
/// `Set-Cookie` line gets rejected outright: over plain HTTP, a `Set-Cookie`
/// carrying `Secure` is discarded entirely, and (once the `__Host-` prefix
/// is in use) that prefix requires `Secure` + `Path=/` + no `Domain` on the
/// same `Set-Cookie` — missing any one discards the whole line. Either way
/// the cookie is never cleared: "I clicked log out but the cookie is still
/// there."
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
        // Strictly below the cap (created_at + 90d == now + 1d), so this
        // exercises the clamp itself rather than the "already at cap"
        // short-circuit.
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

    /// Proves the throttle check runs *before* the cap check: even a
    /// session pinned at the absolute cap must not write on every request,
    /// only once the throttle window has elapsed.
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

    /// The most important assertion in this task: it turns "must not log the
    /// raw token" into an executable constraint rather than a convention
    /// that could silently rot.
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
