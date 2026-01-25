use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

use crate::models::User;

const USER_ID_KEY: &str = "user_id";
const USERNAME_KEY: &str = "username";

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
}

impl AuthUser {
    pub async fn login(session: &Session, user: &User) -> Result<(), tower_sessions::session::Error> {
        session.insert(USER_ID_KEY, &user.id).await?;
        session.insert(USERNAME_KEY, &user.username).await?;
        Ok(())
    }

    pub async fn logout(session: &Session) -> Result<(), tower_sessions::session::Error> {
        session.flush().await
    }

    pub async fn from_session(session: &Session) -> Option<Self> {
        let id = session.get::<String>(USER_ID_KEY).await.ok()??;
        let username = session.get::<String>(USERNAME_KEY).await.ok()??;
        Some(Self { id, username })
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AuthRedirect;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRedirect)?;

        AuthUser::from_session(&session).await.ok_or(AuthRedirect)
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
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Session error"))?;

        Ok(OptionalAuthUser(AuthUser::from_session(&session).await))
    }
}
