use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use axum_extra::extract::CookieJar;

use crate::models::UserRole;
use crate::repositories::{SessionRepository, UserRepository};
use crate::session::get_session_token;

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub role: UserRole,
    pub session_token: String,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role.is_admin()
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AuthRedirect;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRedirect)?;

        let token = get_session_token(&jar).ok_or(AuthRedirect)?;

        let Extension(session_repo) =
            Extension::<SessionRepository>::from_request_parts(parts, state)
                .await
                .map_err(|_| AuthRedirect)?;

        let user_id = session_repo
            .find_valid(&token)
            .await
            .map_err(|_| AuthRedirect)?
            .ok_or(AuthRedirect)?;

        let Extension(user_repo) = Extension::<UserRepository>::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRedirect)?;

        let user = user_repo
            .find_by_id(&user_id)
            .await
            .map_err(|_| AuthRedirect)?
            .ok_or(AuthRedirect)?;

        Ok(AuthUser {
            id: user.id,
            username: user.username,
            role: user.role,
            session_token: token,
        })
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
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Cookie error"))?;

        let token = match get_session_token(&jar) {
            Some(t) => t,
            None => return Ok(OptionalAuthUser(None)),
        };

        let Extension(session_repo) =
            Extension::<SessionRepository>::from_request_parts(parts, state)
                .await
                .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Session error"))?;

        let user_id = match session_repo.find_valid(&token).await {
            Ok(Some(uid)) => uid,
            _ => return Ok(OptionalAuthUser(None)),
        };

        let Extension(user_repo) = Extension::<UserRepository>::from_request_parts(parts, state)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Session error"))?;

        let user = match user_repo.find_by_id(&user_id).await {
            Ok(Some(u)) => u,
            _ => return Ok(OptionalAuthUser(None)),
        };

        Ok(OptionalAuthUser(Some(AuthUser {
            id: user.id,
            username: user.username,
            role: user.role,
            session_token: token,
        })))
    }
}

// Admin user extractor - requires admin role, returns 403 if not admin
#[derive(Clone, Debug)]
pub struct AdminUser(pub AuthUser);

impl std::ops::Deref for AdminUser {
    type Target = AuthUser;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for AdminUser
where
    S: Send + Sync,
{
    type Rejection = AdminOrAuthRedirect;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state)
            .await
            .map_err(|_| AdminOrAuthRedirect::Auth)?;

        if user.is_admin() {
            Ok(AdminUser(user))
        } else {
            Err(AdminOrAuthRedirect::Forbidden)
        }
    }
}

pub enum AdminOrAuthRedirect {
    Auth,
    Forbidden,
}

impl IntoResponse for AdminOrAuthRedirect {
    fn into_response(self) -> Response {
        match self {
            AdminOrAuthRedirect::Auth => Redirect::to("/auth/login").into_response(),
            AdminOrAuthRedirect::Forbidden => {
                (StatusCode::FORBIDDEN, "Admin access required").into_response()
            }
        }
    }
}
