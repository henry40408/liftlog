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
}
