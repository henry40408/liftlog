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
        let cookie = create_session_cookie("tok", false);
        assert_eq!(cookie.secure(), Some(false));
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
}
