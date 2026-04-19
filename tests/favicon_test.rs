mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
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
