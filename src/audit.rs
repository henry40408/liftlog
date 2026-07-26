//! Session lifecycle audit logging (OWASP Session Management Cheat Sheet,
//! *Logging Sessions Life Cycle*).
//!
//! Session creation, renewal, destruction, expiry and rejection are emitted
//! as structured `tracing` events under the `liftlog::audit` target; request-scoped
//! events carry `client_ip`, `user_agent` and `path`, while the hourly background
//! sweep's expiry event is not request-scoped and reports only a `count`, so an
//! operator piping `LOG_FORMAT=json` into a log collector can reconstruct a
//! session's life cycle and correlate it with the requests that drove it.
//!
//! OWASP is explicit that the session identifier itself must never be
//! written to a log — a leaked log line must not be equivalent to a leaked
//! cookie. Every event therefore carries `session_fp`, a salted SHA-256
//! fingerprint of the token (see [`crate::session::token_fingerprint`])
//! rather than the token itself. The salt (`AppState::log_salt`) is
//! generated fresh at process startup, so fingerprints let events be
//! correlated *within* one process's lifetime but deliberately do NOT
//! correlate across restarts — that would require persisting the salt,
//! which is unnecessary complexity for what OWASP actually requires (no
//! raw-token disclosure, not cross-restart correlation).

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::net::IpAddr;

use crate::config::TrustedProxyHeader;
use crate::state::AppState;

/// Everything the audit log needs about the request that triggered a
/// session lifecycle event.
#[derive(Clone, Debug)]
pub struct AuditContext {
    pub client_ip: IpAddr,
    pub user_agent: Option<String>,
    pub path: String,
}

/// A hostile client can send a multi-kilobyte `User-Agent`; without a cap a
/// single request could bloat every log line derived from it.
const MAX_USER_AGENT_LEN: usize = 256;

impl AuditContext {
    /// Builds the context from raw request pieces. Used directly by
    /// `sliding_session_middleware` (which holds a `Request`, not
    /// `AppState`); the `FromRequestParts` impl below delegates here.
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

/// Truncates to at most `max_chars` chars on a char boundary — `s` may be a
/// hostile, non-ASCII `User-Agent`, so a byte-index truncation could split a
/// multi-byte UTF-8 sequence and panic.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

impl FromRequestParts<AppState> for AuditContext {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // `sliding_session_middleware` already built one for every request that
        // carried a session token, so reuse it rather than recomputing the same
        // client-IP resolution and `User-Agent` truncation. The fallback is not
        // dead: the anonymous routes that take this extractor (login and first-user
        // setup) have no session token, so the middleware never built one for them.
        if let Some(ctx) = parts.extensions.get::<Self>() {
            return Ok(ctx.clone());
        }

        Ok(Self::from_request_pieces(
            &parts.extensions,
            &parts.headers,
            parts.uri.path(),
            state.trusted_proxy_header,
            &state.trusted_proxies,
        ))
    }
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

/// Emitted for a bulk delete (password change, "log out other devices",
/// admin user delete). Deliberately has no `session_fp`: a bulk delete has
/// no single session to name, and conflating the surviving actor session
/// (the one that issued the request) with the sessions actually destroyed
/// would make the log lie about which session died. `actor_session_fp`
/// identifies who performed the action, `count` says how many rows died.
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

/// Emitted by the hourly background sweep, not by a request, so it has no
/// `client_ip` / `user_agent` / `path` and no per-session fingerprint: the
/// sweep deletes in bulk without reading the tokens back, and fetching them
/// purely to fingerprint them would add a query per pass for no security
/// benefit. `count` is what an operator actually needs — a sudden spike is
/// the signal worth alerting on.
pub fn sessions_expired_sweep(count: usize) {
    tracing::info!(
        target: "liftlog::audit",
        event = "session.expired",
        count,
        reason = "sweep",
        "expired sessions retired by the background sweep"
    );
}

/// `debug`, not `info`: liftlog is internet-facing, and scanners hammering
/// it with random cookie values would otherwise drown the genuinely useful
/// lifecycle events (created/renewed/destroyed/expired) in noise. An
/// operator who wants to see rejected tokens sets `RUST_LOG` to include
/// `debug`.
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

    /// The extractor must reuse the context `sliding_session_middleware`
    /// already put in the request extensions, not rebuild its own. Proven by
    /// making the two disagree: the stored context carries values the request's
    /// own headers and URI would never produce, so a rebuild is detectable.
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

    /// Documents that `client_ip`'s "no peer" fallback (loopback) is what
    /// audit logs will record under `oneshot`-style calls that never attach
    /// `ConnectInfo`, and pins the delegation to `crate::net::client_ip`.
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

    /// `HeaderValue::to_str` itself rejects non-ASCII bytes (it only yields
    /// visible-ASCII values), so a hostile multi-byte UA never reaches
    /// `AuditContext::user_agent` through the real header-parsing path —
    /// that path is exercised by the tests above. This test instead pins
    /// the truncation helper itself: it must slice on a char boundary, not
    /// a byte index, or a long non-ASCII string would panic by splitting a
    /// multi-byte UTF-8 sequence.
    #[test]
    fn truncate_chars_handles_non_ascii_multi_byte_input_without_panicking() {
        let ua: String = std::iter::repeat_n('台', 5000).collect();
        let truncated = truncate_chars(&ua, MAX_USER_AGENT_LEN);
        assert!(truncated.chars().count() <= MAX_USER_AGENT_LEN);
    }
}
