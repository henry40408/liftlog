use axum::{
    extract::{FromRequestParts, Request, State},
    http::{request::Parts, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use axum_extra::extract::CookieJar;

use crate::models::UserRole;
use crate::repositories::{SessionRepository, UserRepository};
use crate::session::{create_session_cookie, get_session_token};

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

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AuthRedirect;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let validated = parts
            .extensions
            .get::<ValidatedSession>()
            .cloned()
            .ok_or(AuthRedirect)?;

        let Extension(user_repo) = Extension::<UserRepository>::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRedirect)?;

        let user = user_repo
            .find_by_id(&validated.user_id)
            .await
            .map_err(|_| AuthRedirect)?
            .ok_or(AuthRedirect)?;

        Ok(AuthUser {
            id: user.id,
            username: user.username,
            role: user.role,
            session_token: validated.session_token,
        })
    }
}

/// Produced by `sliding_session_middleware` for every request that arrives
/// with a valid session cookie. Extractors downstream read this from
/// request extensions instead of re-hitting the database.
#[derive(Clone, Debug)]
pub struct ValidatedSession {
    pub user_id: String,
    pub session_token: String,
}

/// Axum middleware that validates the session cookie, slides its expiry
/// when the touch throttle has elapsed, and (on touch) re-issues the
/// cookie with a fresh `Max-Age`. Applied globally; requests without a
/// cookie pass through untouched.
pub async fn sliding_session_middleware(
    State(session_repo): State<SessionRepository>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let token = get_session_token(&jar);
    let mut should_refresh_cookie: Option<String> = None;

    if let Some(tok) = token.as_deref() {
        match session_repo.validate_and_touch(tok).await {
            Ok(Some(outcome)) => {
                request.extensions_mut().insert(ValidatedSession {
                    user_id: outcome.user_id,
                    session_token: tok.to_string(),
                });
                if outcome.new_expires_at.is_some() {
                    should_refresh_cookie = Some(tok.to_string());
                }
            }
            Ok(None) => {
                // Invalid / expired token: do not insert ValidatedSession. The
                // downstream extractor (AuthUser) will redirect to /auth/login.
            }
            Err(e) => {
                tracing::warn!(error = ?e, "sliding_session_middleware: validate_and_touch failed");
            }
        }
    }

    let mut response = next.run(request).await;

    if let Some(tok) = should_refresh_cookie {
        // Skip the refresh if the handler already emitted a `session=...`
        // Set-Cookie (e.g. logout's removal cookie). Appending after it
        // would let the refreshed cookie override the removal in the
        // browser.
        let cookie_prefix = format!("{}=", crate::session::SESSION_COOKIE_NAME);
        let already_set = response
            .headers()
            .get_all(axum::http::header::SET_COOKIE)
            .iter()
            .any(|v| {
                v.to_str()
                    .ok()
                    .is_some_and(|s| s.trim_start().starts_with(&cookie_prefix))
            });
        if !already_set {
            let cookie = create_session_cookie(&tok);
            let header_value = cookie
                .to_string()
                .parse()
                .expect("session cookie serialises to a valid header value");
            response
                .headers_mut()
                .append(axum::http::header::SET_COOKIE, header_value);
        }
    }

    response
}

pub struct AuthRedirect;

impl IntoResponse for AuthRedirect {
    fn into_response(self) -> Response {
        Redirect::to("/auth/login").into_response()
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
