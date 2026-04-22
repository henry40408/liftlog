use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;

pub const SESSION_COOKIE_NAME: &str = "session";

/// How long a session survives without activity. A request within this
/// window (and outside the touch throttle) slides the expiry forward.
pub const SESSION_IDLE_TTL_SECS: i64 = 60 * 60 * 24 * 7; // 7 days

/// Minimum gap between two consecutive `last_touched_at` writes for the
/// same session. Keeps write load to at most one UPDATE per session per hour.
pub const SESSION_TOUCH_THROTTLE_SECS: i64 = 60 * 60; // 1 hour

pub fn create_session_cookie(token: &str) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, token.to_string()))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::seconds(SESSION_IDLE_TTL_SECS))
        .build()
}

pub fn get_session_token(jar: &CookieJar) -> Option<String> {
    jar.get(SESSION_COOKIE_NAME)
        .map(|cookie| cookie.value().to_string())
}

pub fn remove_session_cookie() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .max_age(time::Duration::ZERO)
        .build()
}
