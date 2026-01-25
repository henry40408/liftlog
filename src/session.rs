use axum_extra::extract::cookie::{Cookie, Key, SignedCookieJar};
use serde::{Deserialize, Serialize};

pub const SESSION_COOKIE_NAME: &str = "session";

#[derive(Clone)]
pub struct SessionKey(pub Key);

impl SessionKey {
    pub fn generate() -> Self {
        Self(Key::generate())
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Key::try_from(bytes).ok().map(Self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub user_id: String,
    pub username: String,
}

impl SessionData {
    pub fn new(user_id: String, username: String) -> Self {
        Self { user_id, username }
    }

    pub fn to_cookie_value(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_cookie_value(value: &str) -> Option<Self> {
        serde_json::from_str(value).ok()
    }
}

pub fn create_session_cookie(data: &SessionData) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, data.to_cookie_value()))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::days(7))
        .build()
}

pub fn get_session_from_jar(jar: &SignedCookieJar) -> Option<SessionData> {
    jar.get(SESSION_COOKIE_NAME)
        .and_then(|cookie| SessionData::from_cookie_value(cookie.value()))
}

pub fn remove_session_cookie() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .max_age(time::Duration::ZERO)
        .build()
}
