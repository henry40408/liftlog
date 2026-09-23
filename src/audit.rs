//! Structured audit events (session lifecycle, auth failures, CSRF
//! rejections) under the `liftlog::audit` tracing target.
//!
//! Raw session tokens are never logged; events carry `session_fp`, a salted
//! fingerprint ([`crate::session::token_fingerprint`]). The salt is per
//! process, so fingerprints correlate within one run but not across restarts.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::net::IpAddr;

use crate::config::TrustedProxyHeader;
use crate::state::AppState;

#[derive(Clone, Debug)]
pub struct AuditContext {
    pub client_ip: IpAddr,
    pub user_agent: Option<String>,
    pub path: String,
}

// Caps on attacker-controlled, unvalidated fields so one request can't bloat
// log lines.
const MAX_USER_AGENT_LEN: usize = 256;
const MAX_USERNAME_LEN: usize = 256;
const MAX_ORIGIN_LEN: usize = 256;

impl AuditContext {
    /// For callers holding a `Request` rather than `AppState`
    /// (`sliding_session_middleware`).
    pub fn from_request_pieces(
        extensions: &axum::http::Extensions,
        headers: &axum::http::HeaderMap,
        path: &str,
        header: TrustedProxyHeader,
        trusted_proxies: &[IpAddr],
    ) -> Self {
        let peer = extensions
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|ci| ci.0.ip());
        let client_ip = crate::net::client_ip(peer, headers, header, trusted_proxies);

        let user_agent = headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|ua| truncate_chars(ua, MAX_USER_AGENT_LEN));

        Self {
            client_ip,
            user_agent,
            path: path.to_string(),
        }
    }
}

/// Char-based, so non-ASCII input can't panic on a byte boundary.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

// The `let` bindings in the `auth.*`/`csrf.*` emitters are for `llvm-cov`:
// calls inlined into `tracing!` fields report as uncovered. Don't inline them.

impl FromRequestParts<AppState> for AuditContext {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> {
        // Reuse the middleware's context; token-less requests (login, setup)
        // have none, so fall back to building one.
        let ctx = match parts.extensions.get::<Self>() {
            Some(ctx) => ctx.clone(),
            None => Self::from_request_pieces(
                &parts.extensions,
                &parts.headers,
                parts.uri.path(),
                state.trusted_proxy_header,
                &state.trusted_proxies,
            ),
        };

        std::future::ready(Ok(ctx))
    }
}

/// `warn`: the primary brute-force signal. The attempted `username` is logged
/// to identify the targeted account, accepting that a password typed into the
/// username field lands in the log. `backoff_ms` is the per-account delay
/// applied to this attempt.
pub fn login_failed(ctx: &AuditContext, username: &str, backoff_ms: u64) {
    let username = truncate_chars(username, MAX_USERNAME_LEN);
    let user_agent = ctx.user_agent.as_deref();
    tracing::warn!(
        target: "liftlog::audit",
        event = "auth.login.failed",
        username,
        backoff_ms,
        client_ip = %ctx.client_ip,
        user_agent,
        path = %ctx.path,
        "login failed"
    );
}

/// Refused by the per-IP limiter before any credential was checked.
pub fn login_throttled(ctx: &AuditContext, username: &str) {
    let username = truncate_chars(username, MAX_USERNAME_LEN);
    let user_agent = ctx.user_agent.as_deref();
    tracing::warn!(
        target: "liftlog::audit",
        event = "auth.login.throttled",
        username,
        client_ip = %ctx.client_ip,
        user_agent,
        path = %ctx.path,
        "login throttled"
    );
}

/// Wrong password on an authenticated re-check (password change, admin
/// promote/delete) — where a stolen-cookie holder would guess. `action` names
/// the route; one event family so alerting sees one signal.
pub fn reauth_failed(ctx: &AuditContext, actor_session_fp: &str, user_id: &str, action: &str) {
    let user_agent = ctx.user_agent.as_deref();
    tracing::warn!(
        target: "liftlog::audit",
        event = "auth.reauth.failed",
        actor_session_fp,
        user_id,
        action,
        client_ip = %ctx.client_ip,
        user_agent,
        path = %ctx.path,
        "re-authentication rejected: password incorrect"
    );
}

/// Refused by the per-user throttle, whose budget is shared across routes.
pub fn reauth_throttled(ctx: &AuditContext, actor_session_fp: &str, user_id: &str, action: &str) {
    let user_agent = ctx.user_agent.as_deref();
    tracing::warn!(
        target: "liftlog::audit",
        event = "auth.reauth.throttled",
        actor_session_fp,
        user_id,
        action,
        client_ip = %ctx.client_ip,
        user_agent,
        path = %ctx.path,
        "re-authentication throttled"
    );
}

pub fn session_created(
    ctx: &AuditContext,
    session_fp: &str,
    user_id: &str,
    username: &str,
    reason: &str,
) {
    tracing::info!(
        target: "liftlog::audit",
        event = "session.created",
        session_fp,
        user_id,
        username,
        client_ip = %ctx.client_ip,
        user_agent = ctx.user_agent.as_deref(),
        path = %ctx.path,
        reason,
        "session created"
    );
}

pub fn session_renewed(ctx: &AuditContext, session_fp: &str, user_id: &str, username: &str) {
    tracing::info!(
        target: "liftlog::audit",
        event = "session.renewed",
        session_fp,
        user_id,
        username,
        client_ip = %ctx.client_ip,
        user_agent = ctx.user_agent.as_deref(),
        path = %ctx.path,
        "session renewed"
    );
}

pub fn session_destroyed(ctx: &AuditContext, session_fp: &str, user_id: &str, reason: &str) {
    tracing::info!(
        target: "liftlog::audit",
        event = "session.destroyed",
        session_fp,
        user_id,
        client_ip = %ctx.client_ip,
        user_agent = ctx.user_agent.as_deref(),
        path = %ctx.path,
        reason,
        "session destroyed"
    );
}

/// Bulk delete. No `session_fp`: `actor_session_fp` is who acted, not a
/// session that died.
pub fn sessions_destroyed_bulk(
    ctx: &AuditContext,
    actor_session_fp: &str,
    user_id: &str,
    count: usize,
    reason: &str,
) {
    tracing::info!(
        target: "liftlog::audit",
        event = "session.destroyed",
        actor_session_fp,
        user_id,
        count,
        client_ip = %ctx.client_ip,
        user_agent = ctx.user_agent.as_deref(),
        path = %ctx.path,
        reason,
        "sessions destroyed (bulk)"
    );
}

pub fn session_expired(ctx: &AuditContext, session_fp: &str, reason: &str) {
    tracing::info!(
        target: "liftlog::audit",
        event = "session.expired",
        session_fp,
        client_ip = %ctx.client_ip,
        user_agent = ctx.user_agent.as_deref(),
        path = %ctx.path,
        reason,
        "session expired"
    );
}

/// From the background sweep: no request context, no per-session fingerprint.
pub fn sessions_expired_sweep(count: usize) {
    tracing::info!(
        target: "liftlog::audit",
        event = "session.expired",
        count,
        reason = "sweep",
        "expired sessions retired by the background sweep"
    );
}

/// `debug`: scanners replaying random cookies would drown the useful events.
pub fn session_rejected(ctx: &AuditContext, session_fp: &str) {
    tracing::debug!(
        target: "liftlog::audit",
        event = "session.rejected",
        session_fp,
        client_ip = %ctx.client_ip,
        user_agent = ctx.user_agent.as_deref(),
        path = %ctx.path,
        reason = "unknown_token",
        "session rejected"
    );
}

/// A request refused by the CSRF guard ([`crate::middleware::csrf`]). `warn`:
/// either an attack or a misconfigured proxy. `reason` is `sec_fetch_site`
/// (browser-declared) or `origin_fallback` (`Origin`/`Host` mismatch — suspect
/// the proxy first).
pub fn csrf_rejected(ctx: &AuditContext, reason: &str, method: &str, origin: Option<&str>) {
    let origin = origin.map(|o| truncate_chars(o, MAX_ORIGIN_LEN));
    let user_agent = ctx.user_agent.as_deref();
    tracing::warn!(
        target: "liftlog::audit",
        event = "csrf.rejected",
        reason,
        method,
        origin,
        client_ip = %ctx.client_ip,
        user_agent,
        path = %ctx.path,
        "cross-site request rejected"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_context_truncates_a_hostile_user_agent() {
        let mut headers = axum::http::HeaderMap::new();
        let ua = "a".repeat(5000);
        headers.insert(axum::http::header::USER_AGENT, ua.parse().unwrap());
        let extensions = axum::http::Extensions::new();

        let ctx = AuditContext::from_request_pieces(
            &extensions,
            &headers,
            "/",
            TrustedProxyHeader::None,
            &[],
        );

        let got = ctx.user_agent.expect("user agent should be present");
        assert!(got.chars().count() <= MAX_USER_AGENT_LEN);
    }

    /// The stored context disagrees with the request, so a rebuild shows.
    #[tokio::test]
    async fn audit_context_extractor_prefers_the_one_the_middleware_built() {
        let pool = crate::db::create_memory_pool().expect("memory pool");
        let state = AppState {
            user_repo: crate::repositories::UserRepository::new(pool.clone()),
            exercise_repo: crate::repositories::ExerciseRepository::new(pool.clone()),
            workout_repo: crate::repositories::WorkoutRepository::new(pool.clone()),
            session_repo: crate::repositories::SessionRepository::new(pool),
            login_rate_limiter: std::sync::Arc::new(crate::rate_limit::RateLimiter::new(
                5,
                std::time::Duration::from_secs(60),
            )),
            login_backoff: std::sync::Arc::new(crate::rate_limit::FailureBackoff::new(
                3,
                std::time::Duration::ZERO,
                std::time::Duration::ZERO,
                std::time::Duration::from_secs(60),
            )),
            sensitive_action_rate_limiter: std::sync::Arc::new(
                crate::rate_limit::RateLimiter::new(5, std::time::Duration::from_secs(900)),
            ),
            trusted_proxy_header: TrustedProxyHeader::None,
            trusted_proxies: std::sync::Arc::new(vec![]),
            cookie_secure: false,
            hsts_max_age: 0,
            hsts_include_subdomains: false,
            log_salt: std::sync::Arc::new([0u8; 32]),
        };

        let stored = AuditContext {
            client_ip: "203.0.113.7".parse().unwrap(),
            user_agent: Some("middleware-built".to_string()),
            path: "/built-by-middleware".to_string(),
        };

        let request = axum::http::Request::builder()
            .uri("/rebuilt-by-extractor")
            .header(axum::http::header::USER_AGENT, "rebuilt-by-extractor")
            .extension(stored)
            .body(())
            .unwrap();
        let (mut parts, ()) = request.into_parts();

        let ctx = AuditContext::from_request_parts(&mut parts, &state)
            .await
            .expect("extractor is infallible");

        assert_eq!(ctx.path, "/built-by-middleware", "path was rebuilt");
        assert_eq!(
            ctx.user_agent.as_deref(),
            Some("middleware-built"),
            "user_agent was rebuilt"
        );
        assert_eq!(
            ctx.client_ip.to_string(),
            "203.0.113.7",
            "client_ip was rebuilt"
        );
    }

    #[test]
    fn audit_context_user_agent_is_none_when_absent() {
        let headers = axum::http::HeaderMap::new();
        let extensions = axum::http::Extensions::new();

        let ctx = AuditContext::from_request_pieces(
            &extensions,
            &headers,
            "/",
            TrustedProxyHeader::None,
            &[],
        );

        assert!(ctx.user_agent.is_none());
    }

    #[test]
    fn audit_context_falls_back_to_loopback_without_connect_info() {
        let headers = axum::http::HeaderMap::new();
        let extensions = axum::http::Extensions::new();

        let ctx = AuditContext::from_request_pieces(
            &extensions,
            &headers,
            "/",
            TrustedProxyHeader::None,
            &[],
        );

        assert_eq!(ctx.client_ip, "127.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn audit_context_uses_the_connect_info_peer() {
        let headers = axum::http::HeaderMap::new();
        let mut extensions = axum::http::Extensions::new();
        let peer: std::net::SocketAddr = "203.0.113.9:1234".parse().unwrap();
        extensions.insert(axum::extract::ConnectInfo(peer));

        let ctx = AuditContext::from_request_pieces(
            &extensions,
            &headers,
            "/",
            TrustedProxyHeader::None,
            &[],
        );

        assert_eq!(ctx.client_ip, "203.0.113.9".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn truncate_chars_handles_non_ascii_multi_byte_input_without_panicking() {
        let ua: String = std::iter::repeat_n('台', 5000).collect();
        let truncated = truncate_chars(&ua, MAX_USER_AGENT_LEN);
        assert!(truncated.chars().count() <= MAX_USER_AGENT_LEN);
    }
}
