use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use axum_extra::extract::cookie::SignedCookieJar;

use crate::models::User;
use crate::session::{
    create_session_cookie, get_session_from_jar, remove_session_cookie, SessionData, SessionKey,
};

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
}

impl AuthUser {
    pub fn login(jar: SignedCookieJar, user: &User) -> SignedCookieJar {
        let data = SessionData::new(user.id.clone(), user.username.clone());
        jar.add(create_session_cookie(&data))
    }

    pub fn logout(jar: SignedCookieJar) -> SignedCookieJar {
        jar.remove(remove_session_cookie())
    }

    pub fn from_jar(jar: &SignedCookieJar) -> Option<Self> {
        get_session_from_jar(jar).map(|data| Self {
            id: data.user_id,
            username: data.username,
        })
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AuthRedirect;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Extension(key) = Extension::<SessionKey>::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRedirect)?;

        let jar = SignedCookieJar::from_headers(&parts.headers, key.0);

        AuthUser::from_jar(&jar).ok_or(AuthRedirect)
    }
}

pub struct AuthRedirect;

impl IntoResponse for AuthRedirect {
    fn into_response(self) -> Response {
        Redirect::to("/auth/login").into_response()
    }
}

// Optional auth - doesn't redirect, just returns None if not logged in
pub struct OptionalAuthUser(pub Option<AuthUser>);

#[async_trait]
impl<S> FromRequestParts<S> for OptionalAuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Extension(key) = Extension::<SessionKey>::from_request_parts(parts, state)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Session error"))?;

        let jar = SignedCookieJar::from_headers(&parts.headers, key.0);

        Ok(OptionalAuthUser(AuthUser::from_jar(&jar)))
    }
}
