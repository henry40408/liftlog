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
    /// Throttles the authenticated routes that verify a password before
    /// acting — the password change, and the admin promote/delete
    /// confirmations. Keyed by user id: those requests are authenticated, so
    /// the account under attack is known exactly, and an IP key would let one
    /// stolen session buy a fresh budget from every source address.
    ///
    /// One shared budget across all of them on purpose. They are the same
    /// question from an attacker's point of view — "what is this account's
    /// password?" — so letting a guesser move to another route for a fresh
    /// allowance would make the limit decorative.
    ///
    /// Configured in `main` with a far longer window than login's 60 seconds,
    /// because the two defend against different things. Login has to stay
    /// usable for a person who mistypes and retries immediately; changing a
    /// password is a rare, deliberate act, so five attempts per 15 minutes is
    /// generous for the legitimate case while leaving an attacker with a
    /// stolen session only ~480 guesses a day against the current password.
    pub sensitive_action_rate_limiter: Arc<RateLimiter<String>>,
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
