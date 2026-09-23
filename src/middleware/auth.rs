use axum::{
    extract::{FromRequestParts, Request, State},
    http::{StatusCode, request::Parts},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;

use crate::audit::{self, AuditContext};
use crate::models::UserRole;
use crate::repositories::{SessionRepository, ValidateOutcome};
use crate::session::{create_session_cookie, get_session_token};

/// `cookie_secure` keeps re-issued cookies identical to the login-time ones.
#[derive(Clone)]
pub struct SessionLayerState {
    pub session_repo: SessionRepository,
    pub cookie_secure: bool,
    pub log_salt: std::sync::Arc<[u8; 32]>,
    pub trusted_proxy_header: crate::config::TrustedProxyHeader,
    pub trusted_proxies: std::sync::Arc<Vec<std::net::IpAddr>>,
}

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

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> {
        std::future::ready(
            parts
                .extensions
                .get::<ValidatedSession>()
                .cloned()
                .ok_or(AuthRedirect)
                .map(|validated| AuthUser {
                    id: validated.user_id,
                    username: validated.username,
                    role: validated.role,
                    session_token: validated.session_token,
                }),
        )
    }
}

/// Inserted by `sliding_session_middleware` for a valid session; carries the
/// identity so extractors need no `users` lookup.
#[derive(Clone, Debug)]
pub struct ValidatedSession {
    pub user_id: String,
    pub username: String,
    pub role: UserRole,
    pub session_token: String,
}

/// Validates the session cookie, slides its expiry when the touch throttle
/// has elapsed, and then re-issues the cookie. Cookie-less requests pass through.
pub async fn sliding_session_middleware(
    State(layer): State<SessionLayerState>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let token = get_session_token(&jar, layer.cookie_secure);
    let mut should_refresh_cookie: Option<String> = None;
    // Drives the Cache-Control injection below.
    let mut authenticated = false;

    if let Some(tok) = token.as_deref() {
        let ctx = AuditContext::from_request_pieces(
            request.extensions(),
            request.headers(),
            request.uri().path(),
            layer.trusted_proxy_header,
            &layer.trusted_proxies,
        );
        let fp = crate::session::token_fingerprint(tok, layer.log_salt.as_ref());

        match layer.session_repo.validate_and_touch(tok).await {
            Ok(ValidateOutcome::Valid(outcome)) => {
                if outcome.new_expires_at.is_some() {
                    should_refresh_cookie = Some(tok.to_string());
                    audit::session_renewed(&ctx, &fp, &outcome.user_id, &outcome.username);
                }
                authenticated = true;
                request.extensions_mut().insert(ValidatedSession {
                    user_id: outcome.user_id,
                    username: outcome.username,
                    role: outcome.role,
                    session_token: tok.to_string(),
                });
            }
            Ok(ValidateOutcome::ExpiredIdle) => {
                audit::session_expired(&ctx, &fp, "idle");
            }
            Ok(ValidateOutcome::ExpiredAbsolute) => {
                audit::session_expired(&ctx, &fp, "absolute");
            }
            Ok(ValidateOutcome::Unknown) => {
                audit::session_rejected(&ctx, &fp);
            }
            Err(e) => {
                tracing::warn!(error = ?e, "sliding_session_middleware: validate_and_touch failed");
            }
        }

        // Reused by handlers' `AuditContext` extractor.
        request.extensions_mut().insert(ctx);
    }

    let mut response = next.run(request).await;

    if let Some(tok) = should_refresh_cookie {
        // Logout's removal cookie must not be overwritten.
        let suppressed = response
            .extensions()
            .get::<SuppressSessionRefresh>()
            .is_some();
        if !suppressed {
            let cookie = create_session_cookie(&tok, layer.cookie_secure);
            let header_value = cookie
                .to_string()
                .parse()
                .expect("session cookie serialises to a valid header value");
            response
                .headers_mut()
                .append(axum::http::header::SET_COOKIE, header_value);
        }
    }

    // Keep authenticated pages out of caches (OWASP session cheat sheet), but
    // never override a handler's own Cache-Control (e.g. the public favicon).
    if authenticated {
        let headers = response.headers_mut();
        if !headers.contains_key(axum::http::header::CACHE_CONTROL) {
            headers.insert(
                axum::http::header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
            );
            headers.insert(
                axum::http::header::PRAGMA,
                axum::http::HeaderValue::from_static("no-cache"),
            );
        }
    }

    response
}

/// Response extension: don't append a refreshed session cookie.
#[derive(Clone, Copy, Debug)]
pub struct SuppressSessionRefresh;

pub struct AuthRedirect;

impl IntoResponse for AuthRedirect {
    fn into_response(self) -> Response {
        Redirect::to("/auth/login").into_response()
    }
}

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
            .map_err(|_e| AdminOrAuthRedirect::Auth)?;

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
