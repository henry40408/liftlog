use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;

pub const SESSION_COOKIE_NAME: &str = "session";

pub fn create_session_cookie(token: &str) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, token.to_string()))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::days(7))
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
