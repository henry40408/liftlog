//! Password re-entry for sensitive actions, behind the per-user throttle.

use crate::audit::{self, AuditContext};
use crate::error::Result;
use crate::middleware::AuthUser;
use crate::session::token_fingerprint;
use crate::state::AppState;

pub enum ReauthOutcome {
    Verified,
    Throttled,
    WrongPassword,
}

pub struct ReauthAttempt<'a> {
    pub password: &'a str,
    /// Audit name of the action being confirmed.
    pub action: &'a str,
}

/// Throttles current-password guessing with a stolen cookie. Keyed by user id
/// so new IPs don't buy fresh budget; the attempt is charged before verifying.
/// The caller refunds it with `sensitive_action_rate_limiter.release` once the
/// action has succeeded, so failures stay charged.
pub async fn check_password(
    state: &AppState,
    user: &AuthUser,
    audit_ctx: &AuditContext,
    attempt: ReauthAttempt<'_>,
) -> Result<ReauthOutcome> {
    let actor_fp = token_fingerprint(&user.session_token, state.log_salt.as_ref());

    if !state
        .sensitive_action_rate_limiter
        .try_acquire(user.id.clone())
    {
        audit::reauth_throttled(audit_ctx, &actor_fp, &user.id, attempt.action);
        return Ok(ReauthOutcome::Throttled);
    }

    let verified = state
        .user_repo
        .verify_password(&user.username, attempt.password)
        .await?;

    if verified.is_none() {
        audit::reauth_failed(audit_ctx, &actor_fp, &user.id, attempt.action);
        return Ok(ReauthOutcome::WrongPassword);
    }

    Ok(ReauthOutcome::Verified)
}
