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
    let test_app = common::create_test_app_with_key(pool.clone());

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
    let test_app = common::create_test_app_with_key(pool.clone());

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
    let test_app = common::create_test_app_with_key(pool.clone());

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
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create and login a user
    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
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
    let test_app = common::create_test_app_with_key(pool.clone());

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
    let test_app = common::create_test_app_with_key(pool.clone());

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
