use std::net::IpAddr;
use std::sync::Arc;

use crate::config::TrustedProxyHeader;
use crate::rate_limit::RateLimiter;
use crate::repositories::{
    ExerciseRepository, SessionRepository, UserRepository, WorkoutRepository,
};

#[derive(Clone)]
pub struct AppState {
    pub user_repo: UserRepository,
    pub exercise_repo: ExerciseRepository,
    pub workout_repo: WorkoutRepository,
    pub session_repo: SessionRepository,
    /// Throttles `POST /auth/login`, keyed by client IP — the request is
    /// anonymous, so the source address is the only identity available.
    pub login_rate_limiter: Arc<RateLimiter<IpAddr>>,
    /// Throttles `POST /settings/password`, keyed by user id. That route is
    /// liftlog's other password-verification entry point; see
    /// `handlers::settings::change_password` for why the key is the account
    /// and not the address.
    pub password_change_rate_limiter: Arc<RateLimiter<String>>,
    pub trusted_proxy_header: TrustedProxyHeader,
    pub trusted_proxies: Arc<Vec<IpAddr>>,
    pub cookie_secure: bool,
    /// Seconds for the `Strict-Transport-Security` header's `max-age`; `0`
    /// disables the header. See `middleware::security_headers` for why this
    /// defaults off.
    pub hsts_max_age: u64,
    /// Whether the `Strict-Transport-Security` header, when enabled, also
    /// carries `includeSubDomains`.
    pub hsts_include_subdomains: bool,
    /// Per-process random salt for `session_fp` in the audit log. Regenerated
    /// on every restart: events correlate within one process lifetime, not
    /// across restarts. OWASP only requires that the raw token never be
    /// logged, which a per-process salt satisfies with zero configuration.
    pub log_salt: Arc<[u8; 32]>,
}
