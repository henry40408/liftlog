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

/// State bound to the sliding-session middleware layer. Carries the session
/// repository plus the `Secure` flag so cookies re-issued mid-request
/// (on touch) match what `login_submit` / `setup_submit` set at login.
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

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let validated = parts
            .extensions
            .get::<ValidatedSession>()
            .cloned()
            .ok_or(AuthRedirect)?;

        Ok(AuthUser {
            id: validated.user_id,
            username: validated.username,
            role: validated.role,
            session_token: validated.session_token,
        })
    }
}

/// Produced by `sliding_session_middleware` for every request that arrives
/// with a valid session cookie. Carries the full user identity so the
/// `AuthUser` extractor doesn't need a second `users` lookup per request.
#[derive(Clone, Debug)]
pub struct ValidatedSession {
    pub user_id: String,
    pub username: String,
    pub role: UserRole,
    pub session_token: String,
}

/// Axum middleware that validates the session cookie, slides its expiry
/// when the touch throttle has elapsed, and (on touch) re-issues the
/// cookie with a fresh `Max-Age`. Applied globally; requests without a
/// cookie pass through untouched.
pub async fn sliding_session_middleware(
    State(layer): State<SessionLayerState>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let token = get_session_token(&jar, layer.cookie_secure);
    let mut should_refresh_cookie: Option<String> = None;

    if let Some(tok) = token.as_deref() {
        // Only built when there's actually a token to validate — the audit
        // events are the only consumer, so anonymous requests shouldn't pay
        // for it.
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
    }

    let mut response = next.run(request).await;

    if let Some(tok) = should_refresh_cookie {
        // Skip the refresh if the handler explicitly opted out (e.g. logout,
        // which emits a removal cookie that must not be overwritten).
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

    response
}

/// Response-extension marker that handlers (e.g. `logout`) insert to tell
/// `sliding_session_middleware` not to append a refreshed session cookie
/// to this response.
#[derive(Clone, Copy, Debug)]
pub struct SuppressSessionRefresh;

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
