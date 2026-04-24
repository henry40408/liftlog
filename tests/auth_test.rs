mod common;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use liftlog::models::UserRole;
use tower::ServiceExt;

#[tokio::test]
async fn test_login_page_redirects_to_setup_when_no_users() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/setup");
}

#[tokio::test]
async fn test_setup_page_available_when_no_users() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/setup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_dashboard_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    // Should redirect to login
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_login_valid_credentials() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    // Create a test user
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

    // Should redirect to dashboard on success
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/");

    // Should set a session cookie
    let set_cookie = response.headers().get(header::SET_COOKIE);
    assert!(set_cookie.is_some());
    let cookie_str = set_cookie.unwrap().to_str().unwrap();
    assert!(cookie_str.contains("session="));
}

#[tokio::test]
async fn test_login_invalid_credentials() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    // Create a test user
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=wrongpassword"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return OK with error message (not redirect)
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("Invalid username or password"));
}

#[tokio::test]
async fn test_login_nonexistent_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    // Create a user so we don't get redirected to setup
    common::create_test_user(&pool, "existing", "password", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=nonexistent&password=anypassword"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("Invalid username or password"));
}

#[tokio::test]
async fn test_logout_clears_session() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    // Create and login a user
    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to login
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");

    // Should clear the session cookie (max-age=0 or empty value)
    let set_cookie = response.headers().get(header::SET_COOKIE);
    assert!(set_cookie.is_some());
    let cookie_str = set_cookie.unwrap().to_str().unwrap();
    // Cookie should be cleared (either empty or max-age=0)
    assert!(cookie_str.contains("Max-Age=0") || cookie_str.contains("session=;"));
}

#[tokio::test]
async fn test_setup_creates_admin_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/setup")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=admin&password=adminpass123"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to dashboard after successful setup
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/");

    // Verify user was created with admin role
    let user_repo = liftlog::repositories::UserRepository::new(pool);
    let user = user_repo.find_by_username("admin").await.unwrap();
    assert!(user.is_some());
    assert_eq!(user.unwrap().role, UserRole::Admin);
}

#[tokio::test]
async fn test_setup_redirects_when_users_exist() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    // Create an existing user
    common::create_test_user(&pool, "existing", "password", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/auth/setup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to login when users already exist
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_sliding_session_reissues_cookie_when_throttle_elapsed() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    // Artificially age last_touched_at so the next request slides expiry.
    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let token = session_repo.create(&user.id).await.unwrap();
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE sessions SET last_touched_at = datetime('now', '-2 hours') WHERE token = ?",
            [&token],
        )
        .unwrap();
    }

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, format!("session={}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should reach the dashboard (no redirect).
    assert_ne!(response.status(), StatusCode::SEE_OTHER);

    // And Set-Cookie should have been re-issued with a fresh Max-Age.
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("sliding session should set cookie on touch")
        .to_str()
        .unwrap();
    assert!(set_cookie.starts_with("session="));
    assert!(set_cookie.contains("Max-Age=604800")); // 7 days in seconds
}

#[tokio::test]
async fn test_sliding_session_no_cookie_when_within_throttle() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    // Fresh session: last_touched_at is ~now, so within throttle.
    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let token = session_repo.create(&user.id).await.unwrap();

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, format!("session={}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers().get(header::SET_COOKIE).is_none(),
        "cookie should NOT be re-issued within throttle window"
    );
}

#[tokio::test]
async fn test_logout_does_not_get_overridden_by_sliding_refresh() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    // Age the session so the next request triggers a touch.
    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let token = session_repo.create(&user.id).await.unwrap();
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE sessions SET last_touched_at = datetime('now', '-2 hours') WHERE token = ?",
            [&token],
        )
        .unwrap();
    }

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header(header::COOKIE, format!("session={}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Exactly one Set-Cookie for `session=`, and it must be the removal.
    let session_cookies: Vec<_> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter(|s| s.trim_start().starts_with("session="))
        .collect();
    assert_eq!(
        session_cookies.len(),
        1,
        "logout should emit exactly one session Set-Cookie header, got: {:?}",
        session_cookies
    );
    let only = session_cookies[0];
    assert!(
        only.contains("Max-Age=0"),
        "logout cookie should be the removal (Max-Age=0), got: {}",
        only
    );
}

#[tokio::test]
async fn test_expired_session_redirects_to_login() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let token = session_repo.create(&user.id).await.unwrap();
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE sessions SET expires_at = datetime('now', '-1 hour') WHERE token = ?",
            [&token],
        )
        .unwrap();
    }

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, format!("session={}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_login_page_redirects_to_dashboard_when_already_authenticated() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let session_repo = liftlog::repositories::SessionRepository::new(pool.clone());
    let token = session_repo.create(&user.id).await.unwrap();

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .header(header::COOKIE, format!("session={}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/");
}
