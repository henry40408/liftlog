use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::Cookie;

pub const SESSION_COOKIE_NAME: &str = "session";

/// How long a session survives without activity. A request within this
/// window (and outside the touch throttle) slides the expiry forward.
pub const SESSION_IDLE_TTL_SECS: i64 = 60 * 60 * 24 * 7; // 7 days

/// Minimum gap between two consecutive `last_touched_at` writes for the
/// same session. Keeps write load to at most one UPDATE per session per hour.
pub const SESSION_TOUCH_THROTTLE_SECS: i64 = 60 * 60; // 1 hour

pub fn create_session_cookie(token: &str, secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, token.to_string()))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::seconds(SESSION_IDLE_TTL_SECS))
        .build()
}

pub fn get_session_token(jar: &CookieJar) -> Option<String> {
    jar.get(SESSION_COOKIE_NAME)
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
    Cookie::build((SESSION_COOKIE_NAME, ""))
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
}
