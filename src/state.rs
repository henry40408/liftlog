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
    pub login_rate_limiter: Arc<RateLimiter>,
    pub trusted_proxy_header: TrustedProxyHeader,
    pub trusted_proxies: Arc<Vec<IpAddr>>,
    pub cookie_secure: bool,
    /// Per-process random salt for `session_fp` in the audit log. Regenerated
    /// on every restart: events correlate within one process lifetime, not
    /// across restarts. OWASP only requires that the raw token never be
    /// logged, which a per-process salt satisfies with zero configuration.
    pub log_salt: Arc<[u8; 32]>,
}
