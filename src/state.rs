use std::net::IpAddr;
use std::sync::Arc;

use crate::config::TrustedProxyHeader;
use crate::rate_limit::{FailureBackoff, RateLimiter};
use crate::repositories::{
    ExerciseRepository, SessionRepository, UserRepository, WorkoutRepository,
};

#[derive(Clone)]
pub struct AppState {
    pub user_repo: UserRepository,
    pub exercise_repo: ExerciseRepository,
    pub workout_repo: WorkoutRepository,
    pub session_repo: SessionRepository,
    /// `POST /auth/login`, keyed by client IP.
    pub login_rate_limiter: Arc<RateLimiter<IpAddr>>,
    /// Per-account delay keyed by submitted username; see
    /// `FailureBackoff::for_login`.
    pub login_backoff: Arc<FailureBackoff<String>>,
    /// Password re-checks (password change, admin promote/delete), keyed by
    /// user id so a stolen session can't rotate IPs. One budget shared across
    /// routes; 5 per 15 min in `main` (~480/day).
    pub sensitive_action_rate_limiter: Arc<RateLimiter<String>>,
    pub trusted_proxy_header: TrustedProxyHeader,
    pub trusted_proxies: Arc<Vec<IpAddr>>,
    pub cookie_secure: bool,
    /// HSTS `max-age`; `0` disables the header.
    pub hsts_max_age: u64,
    pub hsts_include_subdomains: bool,
    /// Per-process salt for audit `session_fp`; not persisted.
    pub log_salt: Arc<[u8; 32]>,
}
