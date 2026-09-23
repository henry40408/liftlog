//! Asserts audit events as emitted at real call sites, by capturing `tracing`
//! output.

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

/// An in-memory `tracing` sink; clones share one buffer.
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

/// Installs a process-global subscriber for `liftlog::audit=info`. Relies on
/// nextest's process-per-test; under plain `cargo test` the second install
/// panics.
fn install_capturing_subscriber(writer: CapturingWriter) {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(tracing_subscriber::EnvFilter::new("liftlog::audit=info"))
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("no subscriber should already be installed in this test process");
}

/// Value of `key="value"` or `key=value` in a log line, unquoted.
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

    // A leaked log line must not be as good as a leaked cookie.
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
    // Three sessions; the request uses one, so 2 are destroyed.
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

/// Every password failure is logged, so brute force leaves a trace.
#[tokio::test]
async fn failed_login_emits_an_audit_event_naming_the_attempted_username() {
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
                .body(Body::from("username=testuser&password=wrongpass"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let log = writer.contents();
    assert!(
        log.contains("auth.login.failed"),
        "expected an auth.login.failed event, got: {log}"
    );
    assert_eq!(
        extract_field(&log, "username"),
        Some("testuser"),
        "the event must name the account under attack, got: {log}"
    );
    // The submitted password must never reach the log.
    assert!(
        !log.contains("wrongpass"),
        "the attempted password leaked into the audit log: {log}"
    );
}

/// Unknown username and wrong password log the same event, so the log is no
/// user-enumeration oracle.
#[tokio::test]
async fn failed_login_for_an_unknown_user_is_indistinguishable_in_the_log() {
    let writer = CapturingWriter::default();
    install_capturing_subscriber(writer.clone());

    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    for username in ["testuser", "nosuchuser"] {
        let response = test_app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!(
                        "username={username}&password=wrongpass"
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let log = writer.contents();
    let events: Vec<&str> = log
        .lines()
        .filter(|line| line.contains("auth.login.failed"))
        .collect();
    assert_eq!(
        events.len(),
        2,
        "expected one event per attempt, got: {log}"
    );

    // Strip the timestamp and username; everything else must match.
    let normalise = |line: &str| {
        let without_timestamp = line.split_once(" WARN ").map_or(line, |(_, rest)| rest);
        without_timestamp
            .replace("testuser", "X")
            .replace("nosuchuser", "X")
    };
    assert_eq!(
        normalise(events[0]),
        normalise(events[1]),
        "the unknown-user and wrong-password events must not be distinguishable"
    );
}

/// Lockouts are logged as their own event.
#[tokio::test]
async fn throttled_login_emits_its_own_audit_event() {
    let writer = CapturingWriter::default();
    install_capturing_subscriber(writer.clone());

    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_rate_limit(
        pool.clone(),
        1,
        std::time::Duration::from_secs(60),
    );
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    for expected in [StatusCode::OK, StatusCode::TOO_MANY_REQUESTS] {
        let response = test_app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("username=testuser&password=wrongpass"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }

    let log = writer.contents();
    assert!(
        log.contains("auth.login.throttled"),
        "expected an auth.login.throttled event, got: {log}"
    );
    assert!(
        log.contains("auth.login.failed"),
        "the first (unthrottled) attempt should still log a failure, got: {log}"
    );
}

/// A wrong `current_password` on password change is logged as a failure too.
#[tokio::test]
async fn failed_password_change_emits_an_audit_event() {
    let writer = CapturingWriter::default();
    install_capturing_subscriber(writer.clone());

    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());
    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let token = common::create_session_token(&pool, &user).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/password")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, common::cookie_header(&token))
                .body(Body::from(
                    "current_password=wrongpass&new_password=purple-monkey-dishwasher&confirm_password=purple-monkey-dishwasher",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let log = writer.contents();
    assert!(
        log.contains("auth.reauth.failed"),
        "expected an auth.reauth.failed event, got: {log}"
    );
    assert_eq!(
        extract_field(&log, "user_id"),
        Some(user.id.as_str()),
        "the event must identify the account, got: {log}"
    );
    assert!(
        !log.contains(&token),
        "the raw session token leaked into the audit log: {log}"
    );
    assert!(
        !log.contains("wrongpass"),
        "the attempted password leaked into the audit log: {log}"
    );
}

/// `user_agent` reaches the event, truncated to 256 chars.
#[tokio::test]
async fn audit_events_record_a_truncated_user_agent() {
    let writer = CapturingWriter::default();
    install_capturing_subscriber(writer.clone());

    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let hostile_ua = "M".repeat(5000);
    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::USER_AGENT, &hostile_ua)
                .body(Body::from("username=testuser&password=wrongpass"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let log = writer.contents();
    assert!(log.contains("auth.login.failed"), "got: {log}");
    let logged_ua = extract_field(&log, "user_agent").expect("user_agent field should be present");
    assert_eq!(
        logged_ua.len(),
        256,
        "the 5000-char User-Agent should have been truncated to 256, got {} chars",
        logged_ua.len()
    );
    assert!(
        !log.contains(&hostile_ua),
        "the untruncated User-Agent reached the log"
    );
}

/// Per-account backoff, asserted on the logged `backoff_ms` rather than wall
/// clock, which flakes on slow runners. That the delay is served is covered
/// by the lower-bound timing test in `auth_test`.
#[tokio::test]
async fn login_backoff_climbs_per_account_and_resets_on_success() {
    let writer = CapturingWriter::default();
    install_capturing_subscriber(writer.clone());

    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_login_backoff(
        pool.clone(),
        1,
        std::time::Duration::from_millis(10),
    );
    common::create_test_user(&pool, "victim", "password123", UserRole::User).await;
    common::create_test_user(&pool, "bystander", "password123", UserRole::User).await;

    let login = |username: &str, password: &str| {
        Request::builder()
            .method("POST")
            .uri("/auth/login")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!(
                "username={username}&password={password}"
            )))
            .unwrap()
    };

    let last_backoff = |log: &str| -> u64 {
        log.lines()
            .rfind(|l| l.contains("auth.login.failed"))
            .and_then(|l| extract_field(l, "backoff_ms").map(str::to_string))
            .expect("a failed login should report backoff_ms")
            .parse()
            .expect("backoff_ms should be a number")
    };

    // One free failure, then the delay starts and doubles.
    for expected in [0u64, 10, 20] {
        let response = test_app
            .router
            .clone()
            .oneshot(login("victim", "wrongpass"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            last_backoff(&writer.contents()),
            expected,
            "expected backoff_ms={expected} on this attempt"
        );
    }

    // A different account is untouched by the victim's penalty.
    let response = test_app
        .router
        .clone()
        .oneshot(login("bystander", "wrongpass"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        last_backoff(&writer.contents()),
        0,
        "another account must not inherit the penalty"
    );

    // A correct password clears the penalty.
    let response = test_app
        .router
        .clone()
        .oneshot(login("victim", "password123"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = test_app
        .router
        .oneshot(login("victim", "wrongpass"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        last_backoff(&writer.contents()),
        0,
        "a successful login should have cleared the accumulated penalty"
    );
}

/// The event names the rejecting branch, so an operator can tell a cross-site
/// request (`sec_fetch_site`) from a proxy mangling `Host` (`origin_fallback`).
#[tokio::test]
async fn rejected_cross_site_requests_are_logged_with_the_branch_that_rejected() {
    let writer = CapturingWriter::default();
    install_capturing_subscriber(writer.clone());

    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let response = test_app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workouts")
                .header("sec-fetch-site", "cross-site")
                .header(header::ORIGIN, "https://evil.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let log = writer.contents();
    assert!(
        log.contains("csrf.rejected"),
        "expected a csrf.rejected event, got: {log}"
    );
    assert_eq!(
        extract_field(&log, "reason"),
        Some("sec_fetch_site"),
        "expected the browser-declared branch, got: {log}"
    );
    assert_eq!(extract_field(&log, "method"), Some("POST"));
    assert_eq!(
        extract_field(&log, "origin"),
        Some("https://evil.example.com"),
        "the event must record where the request claimed to come from, got: {log}"
    );
    assert_eq!(extract_field(&log, "path"), Some("/workouts"));

    // No `Sec-Fetch-Site` and no `Host`: the `Origin` fallback rejects.
    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workouts")
                .header(header::ORIGIN, "https://app.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let log = writer.contents();
    assert!(
        log.contains("reason=\"origin_fallback\"") || log.contains("reason=origin_fallback"),
        "expected the Origin-fallback branch to be named, got: {log}"
    );
}

/// `Origin` is attacker-controlled and unbounded.
#[tokio::test]
async fn a_hostile_origin_is_truncated_in_the_audit_log() {
    let writer = CapturingWriter::default();
    install_capturing_subscriber(writer.clone());

    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let origin = format!("https://{}.example.com", "a".repeat(5000));
    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workouts")
                .header("sec-fetch-site", "cross-site")
                .header(header::ORIGIN, &origin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let log = writer.contents();
    let logged = extract_field(&log, "origin").expect("origin field should be present");
    assert_eq!(
        logged.chars().count(),
        256,
        "origin should be capped at 256 chars, got {} chars",
        logged.chars().count()
    );
    assert!(
        !log.contains(&origin),
        "the untruncated origin must not reach the log"
    );
}
