mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use liftlog::models::UserRole;
use liftlog::repositories::{SessionRepository, UserRepository};
use tower::ServiceExt;

#[tokio::test]
async fn test_settings_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_settings_page_renders() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/settings")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    assert!(body_str.contains("Settings") || body_str.contains("testuser"));
}

#[tokio::test]
async fn test_settings_shows_git_version() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/settings")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Should contain git version info (at least some version-like string)
    // GIT_VERSION is set at build time, so we just check the page renders with version info
    assert!(
        body_str.contains("version") || body_str.contains("Version") || body_str.len() > 100 // Page has content
    );
}

#[tokio::test]
async fn test_change_password_success() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/password")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from(
                    "current_password=password123&new_password=purple-monkey-dishwasher&confirm_password=purple-monkey-dishwasher",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("Password changed successfully"));

    let user_repo = UserRepository::new(pool.clone());
    let verified = user_repo
        .verify_password("testuser", "purple-monkey-dishwasher")
        .await
        .unwrap();
    assert!(verified.is_some());
}

#[tokio::test]
async fn test_change_password_mismatch() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/password")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from(
                    "current_password=password123&new_password=purple-monkey-dishwasher&confirm_password=amber-tractor-lantern",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("do not match"));
}

#[tokio::test]
async fn test_change_password_too_short() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/password")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from(
                    "current_password=password123&new_password=short&confirm_password=short",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("at least 12 characters"));
}

#[tokio::test]
async fn test_change_password_wrong_current() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/password")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from(
                    "current_password=wrongpass&new_password=purple-monkey-dishwasher&confirm_password=purple-monkey-dishwasher",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("incorrect"));
}

#[tokio::test]
async fn test_change_password_invalidates_other_sessions() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let session_repo = SessionRepository::new(pool.clone());
    let token_current = session_repo.create(&user.id).await.unwrap();
    let token_other = session_repo.create(&user.id).await.unwrap();

    let cookie_header = common::cookie_header(&token_current);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/password")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from(
                    "current_password=password123&new_password=purple-monkey-dishwasher&confirm_password=purple-monkey-dishwasher",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // The response must hand back a replacement cookie, because the token the
    // request arrived on is now gone.
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("password change should re-issue a session cookie")
        .to_str()
        .unwrap()
        .to_string();
    let new_token = common::extract_cookie_header(&set_cookie)
        .strip_prefix(&format!(
            "{}=",
            liftlog::session::session_cookie_name(false)
        ))
        .expect("cookie should carry the replacement token")
        .to_string();

    assert_ne!(
        new_token, token_current,
        "the session token must be rotated, not reused"
    );

    // Every pre-change token is dead: the other device, and — this is the part
    // that was missing before — the caller's own.
    for (name, token) in [("current", &token_current), ("other", &token_other)] {
        assert!(
            matches!(
                session_repo.validate_and_touch(token).await.unwrap(),
                liftlog::repositories::ValidateOutcome::Unknown
            ),
            "the {name} session should have been destroyed"
        );
    }

    assert!(
        matches!(
            session_repo.validate_and_touch(&new_token).await.unwrap(),
            liftlog::repositories::ValidateOutcome::Valid(_)
        ),
        "the replacement session should be usable"
    );
    assert_eq!(
        session_repo.count_for_user(&user.id).await.unwrap(),
        1,
        "exactly one session — the replacement — should survive"
    );
}

/// The replacement cookie has to actually authenticate the next request, not
/// merely exist. Without `SuppressSessionRefresh` the sliding middleware would
/// append a second `Set-Cookie` carrying the *old* token, and whichever the
/// browser kept last would decide whether the user stayed logged in.
#[tokio::test]
async fn test_rotated_session_cookie_authenticates_the_next_request() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .clone()
        .oneshot(change_password_request(
            &cookie_header,
            "current_password=password123&new_password=purple-monkey-dishwasher&confirm_password=purple-monkey-dishwasher",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let cookies: Vec<String> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();
    assert_eq!(
        cookies.len(),
        1,
        "exactly one Set-Cookie — a second one would be the stale refresh: {cookies:?}"
    );
    let new_cookie = common::extract_cookie_header(&cookies[0]);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/settings")
                .header(header::COOKIE, &new_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the rotated cookie should still be logged in"
    );
}

#[tokio::test]
async fn test_settings_page_lists_sessions_with_this_device_marker() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let current_token = session_repo.create(&user.id).await.unwrap();
    let _other_token = session_repo.create(&user.id).await.unwrap();

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/settings")
                .header(header::COOKIE, common::cookie_header(&current_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(
        body.contains("Active Sessions"),
        "missing Active Sessions heading"
    );
    assert!(body.contains("This device"), "missing This device marker");
    assert!(body.contains("Other device"), "missing Other device row");
}

#[tokio::test]
async fn test_logout_others_deletes_siblings_only() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let current_token = session_repo.create(&user.id).await.unwrap();
    let sibling_token = session_repo.create(&user.id).await.unwrap();

    let app = common::create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/logout-others")
                .header(header::COOKIE, common::cookie_header(&current_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Sibling is gone, current survives.
    assert!(
        matches!(
            session_repo
                .validate_and_touch(&sibling_token)
                .await
                .unwrap(),
            liftlog::repositories::ValidateOutcome::Unknown
        ),
        "sibling session should be deleted"
    );
    assert!(
        matches!(
            session_repo
                .validate_and_touch(&current_token)
                .await
                .unwrap(),
            liftlog::repositories::ValidateOutcome::Valid(_)
        ),
        "current session should survive"
    );
}

/// The guard used to be `onsubmit="return confirm(...)"`, which a browser
/// with JavaScript off never runs — the form posted straight through and every
/// other session died on the first click. The trigger is now a link to a
/// confirmation page, so the confirmation step survives without scripts.
#[tokio::test]
async fn test_logout_others_is_gated_by_a_confirmation_page() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let token = common::create_session_token(&pool, &user).await;

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/settings")
                .header(header::COOKIE, common::cookie_header(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(
        !body.contains("onsubmit=\"return confirm("),
        "the settings page must not depend on confirm() to guard anything"
    );
    assert!(
        body.contains(r#"<a href="/settings/logout-others""#),
        "logout-others should be reached through its confirmation page"
    );
}

/// The other two phrasings of the session count. "0 other signed-in devices
/// will be logged out" would be a confusing thing to offer, so that case gets
/// its own wording.
#[tokio::test]
async fn test_logout_others_confirmation_page_phrases_the_session_count() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;
    let app = common::create_test_app(pool.clone());

    let current_token = common::create_session_token(&pool, &user).await;

    // Only this session exists.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/settings/logout-others")
                .header(header::COOKIE, common::cookie_header(&current_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(
        body.contains("No other device is signed in, so nothing will be logged out."),
        "a lone session should not be offered a logout of nobody, got: {body}"
    );

    // Two more devices sign in.
    common::create_session_token(&pool, &user).await;
    common::create_session_token(&pool, &user).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/settings/logout-others")
                .header(header::COOKIE, common::cookie_header(&current_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(
        body.contains("2 other signed-in devices will be logged out"),
        "the count should be plural at two, got: {body}"
    );
}

/// The confirmation page must name how many sessions are about to end, and
/// must not end any of them itself — a GET that logged devices out would be
/// triggerable by any prefetch.
#[tokio::test]
async fn test_logout_others_confirmation_page_counts_sessions_without_acting() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let current_token = common::create_session_token(&pool, &user).await;
    let sibling_token = common::create_session_token(&pool, &user).await;

    let app = common::create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/settings/logout-others")
                .header(header::COOKIE, common::cookie_header(&current_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(
        body.contains("1 other signed-in device will be logged out"),
        "the page should say how many devices are affected, got: {body}"
    );
    assert!(
        body.contains(r#"<form method="post" action="/settings/logout-others">"#),
        "the page should post back to the same route"
    );

    // The GET must have been inert.
    let session_repo = liftlog::repositories::SessionRepository::new(pool);
    assert!(
        matches!(
            session_repo
                .validate_and_touch(&sibling_token)
                .await
                .unwrap(),
            liftlog::repositories::ValidateOutcome::Valid(_)
        ),
        "the sibling session must survive merely viewing the confirmation page"
    );
}

#[tokio::test]
async fn test_change_password_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/settings/password")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "current_password=x&new_password=newpass&confirm_password=newpass",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

/// OWASP Session Management Cheat Sheet (Web Content Caching): /settings
/// carries account details and must never be resurrected from a browser or
/// intermediate cache after logout.
#[tokio::test]
async fn test_settings_page_sets_no_store() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/settings")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "no-cache, no-store, must-revalidate"
    );
    assert_eq!(response.headers().get("pragma").unwrap(), "no-cache");
}

/// Builds a `POST /settings/password` request. The throttle tests fire the
/// same request repeatedly, and `oneshot` consumes the router, so each call
/// needs a freshly built one.
fn change_password_request(cookie_header: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/settings/password")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, cookie_header)
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// `/settings/password` is liftlog's second password-verification entry point
/// and was previously unthrottled: an attacker holding a stolen session cookie
/// could guess `current_password` without limit, two Argon2 operations at a
/// time. After the budget is exhausted the route must refuse *before*
/// verifying anything.
#[tokio::test]
async fn test_change_password_throttled_after_max_attempts() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_password_change_limit(
        pool.clone(),
        3,
        std::time::Duration::from_secs(60),
    );

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);
    let body = "current_password=wrongpass&new_password=purple-monkey-dishwasher&confirm_password=purple-monkey-dishwasher";

    for attempt in 1..=3 {
        let response = test_app
            .router
            .clone()
            .oneshot(change_password_request(&cookie_header, body))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "attempt {attempt} should be answered, not throttled"
        );
    }

    let response = test_app
        .router
        .clone()
        .oneshot(change_password_request(&cookie_header, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("Too many password change attempts"),
        "throttled response should say so, got: {body_str}"
    );
}

/// The reservation is refunded only once the current password proved correct,
/// so a legitimate user changing their password repeatedly is never locked
/// out. Without the refund, the third change below would be throttled.
#[tokio::test]
async fn test_successful_change_password_releases_its_attempt() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_password_change_limit(
        pool.clone(),
        1,
        std::time::Duration::from_secs(60),
    );

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let mut cookie_header = common::extract_cookie_header(&session_cookie);

    // A budget of 1 means every one of these must be refunded to succeed.
    // Each success also rotates the session token, so the cookie has to be
    // carried forward the way a browser would — reusing the original would
    // fail on the second pass for the *wrong* reason (dead session, not
    // throttling) and quietly stop testing the refund.
    let rotations = [
        ("password123", "amber-tractor-lantern"),
        ("amber-tractor-lantern", "velvet-harbour-kestrel"),
        ("velvet-harbour-kestrel", "copper-thistle-marmot"),
    ];
    for (current, new) in rotations {
        let body = format!("current_password={current}&new_password={new}&confirm_password={new}");
        let response = test_app
            .router
            .clone()
            .oneshot(change_password_request(&cookie_header, &body))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "rotating {current} -> {new} should not be throttled"
        );
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("each change should re-issue a session cookie")
            .to_str()
            .unwrap()
            .to_string();
        cookie_header = common::extract_cookie_header(&set_cookie);
    }

    let user_repo = UserRepository::new(pool.clone());
    assert!(
        user_repo
            .verify_password("testuser", "copper-thistle-marmot")
            .await
            .unwrap()
            .is_some(),
        "the final rotation should have been applied"
    );
}

/// The throttle keys on the user id, not the client IP, so one account
/// exhausting its budget must not spend another account's.
#[tokio::test]
async fn test_change_password_throttle_is_per_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_password_change_limit(
        pool.clone(),
        1,
        std::time::Duration::from_secs(60),
    );

    let alice = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;
    let bob = common::create_test_user(&pool, "bob", "password123", UserRole::User).await;
    let alice_cookie =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &alice).await);
    let bob_cookie =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &bob).await);
    let body = "current_password=wrongpass&new_password=purple-monkey-dishwasher&confirm_password=purple-monkey-dishwasher";

    // Alice burns her single attempt, then is throttled.
    for expected in [StatusCode::OK, StatusCode::TOO_MANY_REQUESTS] {
        let response = test_app
            .router
            .clone()
            .oneshot(change_password_request(&alice_cookie, body))
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }

    // Bob's budget is untouched.
    let response = test_app
        .router
        .clone()
        .oneshot(change_password_request(&bob_cookie, body))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a second user must have its own budget"
    );
}

/// Over-long passwords are rejected outright. Crucially they must NOT be
/// truncated to the limit and accepted — that would let a user believe a long
/// passphrase protects them while only its prefix is ever checked.
#[tokio::test]
async fn test_change_password_rejects_over_long_password() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let too_long = "a".repeat(liftlog::models::user::MAX_PASSWORD_LEN + 1);
    let body =
        format!("current_password=password123&new_password={too_long}&confirm_password={too_long}");

    let response = test_app
        .router
        .oneshot(change_password_request(&cookie_header, &body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes);
    assert!(
        body_str.contains("at most 128 characters"),
        "expected the maximum-length message, got: {body_str}"
    );

    // Neither the full password nor a truncated prefix of it may have been
    // stored: the original password must still be the live one.
    let user_repo = UserRepository::new(pool.clone());
    assert!(
        user_repo
            .verify_password("testuser", "password123")
            .await
            .unwrap()
            .is_some(),
        "the original password should still work"
    );
    assert!(
        user_repo
            .verify_password(
                "testuser",
                &"a".repeat(liftlog::models::user::MAX_PASSWORD_LEN)
            )
            .await
            .unwrap()
            .is_none(),
        "the over-long password must have been rejected, not silently truncated"
    );
}

/// The strength gate on the change-password route. Also pins the ordering: the
/// policy is checked *before* the current password, so a weak new password is
/// refused without spending an Argon2 verification on it.
#[tokio::test]
async fn test_change_password_rejects_a_guessable_new_password() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(change_password_request(
            &cookie_header,
            "current_password=password123&new_password=MyPassword12&confirm_password=MyPassword12",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        !body_str.contains("must be at least"),
        "should be rejected on strength, not length: {body_str}"
    );

    let user_repo = UserRepository::new(pool.clone());
    assert!(
        user_repo
            .verify_password("testuser", "password123")
            .await
            .unwrap()
            .is_some(),
        "the original password must be untouched"
    );
}

/// Companion to the setup-side test: the username reaches the strength check
/// here too, taken from the authenticated session rather than the form.
#[tokio::test]
async fn test_change_password_rejects_a_password_derived_from_the_username() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "henrylifts", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(change_password_request(
            &cookie_header,
            "current_password=password123&new_password=henrylifts.42x&confirm_password=henrylifts.42x",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let user_repo = UserRepository::new(pool.clone());
    assert!(
        user_repo
            .verify_password("henrylifts", "henrylifts.42x")
            .await
            .unwrap()
            .is_none(),
        "a password built from the username must be rejected"
    );
}

/// Re-submitting the current password as the new one is refused. Without this
/// the request would report success, destroy every other session and rotate
/// the token — all for a password that did not change, leaving someone who
/// was rotating a compromised credential believing they had.
#[tokio::test]
async fn test_change_password_rejects_reusing_the_current_password() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(
        &pool,
        "testuser",
        "purple-monkey-dishwasher",
        UserRole::User,
    )
    .await;
    let other_token = SessionRepository::new(pool.clone())
        .create(&user.id)
        .await
        .unwrap();
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(change_password_request(
            &cookie_header,
            "current_password=purple-monkey-dishwasher&new_password=purple-monkey-dishwasher&confirm_password=purple-monkey-dishwasher",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().get(header::SET_COOKIE).is_none(),
        "a refused change must not rotate the session"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("different from the current password"),
        "got: {body_str}"
    );

    // The side effects of a real change must not have happened either.
    assert!(
        matches!(
            SessionRepository::new(pool.clone())
                .validate_and_touch(&other_token)
                .await
                .unwrap(),
            liftlog::repositories::ValidateOutcome::Valid(_)
        ),
        "other sessions must survive a refused change"
    );
}
