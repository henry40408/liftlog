//! Proves that `src/audit.rs`'s events are actually emitted with the claimed
//! shape at real call sites, not just that the pure helper functions it
//! wraps (`token_fingerprint`) behave correctly in isolation.
//!
//! Two invariants are pinned here because mutation testing showed neither
//! was covered by anything else in the suite:
//!   - `login_submit` must log a salted fingerprint of the new session
//!     token, never the raw token itself (OWASP: a leaked log line must not
//!     be equivalent to a leaked cookie).
//!   - `logout_others` must log the *actual* number of sessions destroyed,
//!     not a placeholder.
//!
//! Both are verified by capturing real `tracing` output through a
//! `tracing_subscriber` subscriber installed for the test, rather than by
//! asserting on `audit::*`'s return value (there isn't one) or by calling
//! the module's functions directly (that would just re-test the pure
//! formatting code, not the call sites).

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use liftlog::models::UserRole;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use tracing_subscriber::fmt::MakeWriter;

/// An in-memory `tracing` sink. Cloning shares the same underlying buffer
/// (that's the point — `tracing_subscriber::fmt` clones the `MakeWriter`
/// once per event/span it writes), so the test keeps its own handle to read
/// back what was written.
#[derive(Clone, Default)]
struct CapturingWriter(Arc<Mutex<Vec<u8>>>);

impl CapturingWriter {
    fn contents(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).expect("log output should be UTF-8")
    }
}

impl Write for CapturingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CapturingWriter {
    type Writer = CapturingWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Installs a `tracing` subscriber that writes to `writer` and lets
/// `liftlog::audit` events at `info` (and above) through, then returns a
/// guard-like value: dropping the returned `DefaultGuard` would normally
/// restore the previous subscriber, but this deliberately uses
/// `set_global_default` instead of `with_default`/a `DefaultGuard`.
///
/// Why global rather than thread-local: the events under test
/// (`audit::session_created`, `audit::sessions_destroyed_bulk`) are emitted
/// directly on the async handler's task, not inside `spawn_blocking` — so a
/// thread-local subscriber installed via `tracing::subscriber::with_default`
/// around a `#[tokio::test]`'s (default current-thread-runtime) body would
/// also have worked. `set_global_default` is used instead because it needs
/// no scope-guard bookkeeping around the `.oneshot(...)` call, and because
/// nextest — unlike plain `cargo test` — runs every test in its own process
/// (confirmed by this crate's CLAUDE.md), so one test installing a process
/// -global subscriber can never leak into or collide with another test's.
/// This would NOT be safe under plain `cargo test`, which runs many tests as
/// threads inside one shared process.
fn install_capturing_subscriber(writer: CapturingWriter) {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(tracing_subscriber::EnvFilter::new("liftlog::audit=info"))
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("no subscriber should already be installed in this test process");
}

/// Pulls the value out of a `key="value"` (or bare `key=value`) pair in a
/// captured log line. `tracing_subscriber`'s default field formatter Debug
/// -formats string fields, which wraps them in quotes; this strips those if
/// present so callers get the raw value either way.
fn extract_field<'a>(log: &'a str, field: &str) -> Option<&'a str> {
    let needle = format!("{field}=");
    let start = log.find(&needle)? + needle.len();
    let rest = &log[start..];
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    let end = rest.find(['"', ' ', '\n']).unwrap_or(rest.len());
    Some(&rest[..end])
}

#[tokio::test]
async fn login_emits_session_created_with_a_fingerprint_not_the_raw_token() {
    let writer = CapturingWriter::default();
    install_capturing_subscriber(writer.clone());

    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=password123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("login should set a session cookie")
        .to_str()
        .unwrap()
        .to_string();
    let cookie_header = common::extract_cookie_header(&set_cookie);
    let raw_token = cookie_header
        .strip_prefix(&format!(
            "{}=",
            liftlog::session::session_cookie_name(false)
        ))
        .expect("cookie should carry the session token")
        .to_string();
    assert!(!raw_token.is_empty());

    let log = writer.contents();
    assert!(
        log.contains("session.created"),
        "expected a session.created event, got: {log}"
    );
    assert!(
        log.contains("reason=\"login\"") || log.contains("reason=login"),
        "expected reason=login on the event, got: {log}"
    );

    let fp = extract_field(&log, "session_fp").expect("session_fp field should be present");
    assert_eq!(fp.len(), 16, "session_fp should be 16 hex chars, got: {fp}");
    assert!(
        fp.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "session_fp should be lowercase hex, got: {fp}"
    );

    // The critical assertion: the raw token handed to the browser must never
    // appear anywhere in the captured log output.
    assert!(
        !log.contains(&raw_token),
        "raw session token leaked into the audit log: {log}"
    );
}

#[tokio::test]
async fn logout_others_reports_the_number_of_sessions_actually_destroyed() {
    let writer = CapturingWriter::default();
    install_capturing_subscriber(writer.clone());

    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    // Three sessions total; the request below authenticates with one of
    // them, so exactly 2 should be destroyed by logout-others.
    let token1 = common::create_session_token(&pool, &user).await;
    let _token2 = common::create_session_token(&pool, &user).await;
    let _token3 = common::create_session_token(&pool, &user).await;
    let cookie_header = common::cookie_header(&token1);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/logout-others")
                .header(header::COOKIE, cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("Logged out of all other devices."));

    let log = writer.contents();
    assert!(
        log.contains("session.destroyed"),
        "expected a session.destroyed event, got: {log}"
    );
    assert!(
        log.contains("reason=\"logout_others\"") || log.contains("reason=logout_others"),
        "expected reason=logout_others on the event, got: {log}"
    );
    let count = extract_field(&log, "count").expect("count field should be present");
    assert_eq!(
        count, "2",
        "expected count=2 destroyed sessions, got: {log}"
    );
}
