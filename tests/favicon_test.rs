mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use liftlog::models::UserRole;
use tower::ServiceExt;

#[tokio::test]
async fn test_favicon_svg_returns_ok() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/favicon.svg")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "image/svg+xml"
    );
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "public, max-age=86400"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body.starts_with(b"<svg"));
}

#[tokio::test]
async fn test_apple_touch_icon_returns_ok() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/apple-touch-icon.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "image/png");
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "public, max-age=86400"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    // PNG magic number
    assert_eq!(&body[0..8], b"\x89PNG\r\n\x1a\n");
}

/// Regression guard for the OWASP no-store hardening in
/// `sliding_session_middleware`: an authenticated request still passes
/// through this handler (a logged-in user's browser fetches /favicon.svg
/// like any other asset), and the middleware's `contains_key` guard must
/// leave the handler's own `public, max-age=86400` header untouched rather
/// than overwriting it with `no-cache, no-store, must-revalidate`.
#[tokio::test]
async fn test_favicon_keeps_public_cache_control_for_authenticated_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/favicon.svg")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "public, max-age=86400"
    );
}
