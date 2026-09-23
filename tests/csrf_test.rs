mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use liftlog::models::UserRole;
use liftlog::repositories::WorkoutRepository;
use tower::ServiceExt;

#[tokio::test]
async fn cross_site_post_is_blocked() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie = common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workouts")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie)
                .header("sec-fetch-site", "cross-site")
                .body(Body::from("date=2024-01-15&notes=Leg%20day"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Nothing was created.
    let workout_repo = WorkoutRepository::new(pool);
    let count = workout_repo.count_sessions_by_user(&user.id).await.unwrap();
    assert_eq!(count, 0);
}

/// `same-site` (sibling subdomain, other port) still gets the `SameSite=Lax`
/// cookie, so it is rejected.
#[tokio::test]
async fn same_site_post_is_blocked() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie = common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workouts")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie)
                .header("sec-fetch-site", "same-site")
                .header(header::ORIGIN, "https://other.example.com")
                .header(header::HOST, "app.example.com")
                .body(Body::from("date=2024-01-15&notes=Leg%20day"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let workout_repo = WorkoutRepository::new(pool);
    let count = workout_repo.count_sessions_by_user(&user.id).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn mismatched_origin_is_blocked() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie = common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exercises")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie)
                .header(header::ORIGIN, "https://evil.example.com")
                .header(header::HOST, "localhost")
                .body(Body::from("name=Squat&category=Legs"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// The `Origin` fallback compares the port too: cookies ignore ports, so
/// another port on the same host would otherwise carry the victim's session.
#[tokio::test]
async fn origin_fallback_rejects_an_authority_that_differs_from_host() {
    for (origin, host, why) in [
        (
            "http://localhost:9000",
            "localhost:8080",
            "another port on the same host is another origin",
        ),
        (
            "https://app.example.com:8443",
            "app.example.com",
            "a proxy forwarding `Host` without the browser's port cannot be \
             told apart from the case above",
        ),
        (
            "https://other.example.com",
            "app.example.com",
            "a sibling subdomain is another origin",
        ),
        (
            "null",
            "app.example.com",
            "an opaque origin is never legitimate for a mutation",
        ),
        (
            "not-an-origin",
            "app.example.com",
            "an unparseable Origin cannot be confirmed same-origin",
        ),
    ] {
        let pool = common::setup_test_db();
        let test_app = common::create_test_app_with_session(pool.clone());
        let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
        let cookie =
            common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);

        let response = test_app
            .router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/workouts")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, &cookie)
                    .header(header::ORIGIN, origin)
                    .header(header::HOST, host)
                    .body(Body::from("date=2024-01-15&notes=Leg%20day"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "Origin {origin} / Host {host} must be rejected: {why}"
        );

        let workout_repo = WorkoutRepository::new(pool);
        let count = workout_repo.count_sessions_by_user(&user.id).await.unwrap();
        assert_eq!(
            count, 0,
            "Origin {origin} / Host {host} reached the handler"
        );
    }
}

/// The fallback accepts a LAN install on a non-default port, and a TLS proxy
/// forwarding a scheme- and port-less `Host`.
#[tokio::test]
async fn origin_fallback_passes_an_authority_that_matches_host() {
    for (origin, host) in [
        ("http://192.168.1.5:8080", "192.168.1.5:8080"),
        ("https://app.example.com", "app.example.com"),
    ] {
        let pool = common::setup_test_db();
        let test_app = common::create_test_app_with_session(pool.clone());
        let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
        let cookie =
            common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);

        let response = test_app
            .router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/workouts")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, &cookie)
                    .header(header::ORIGIN, origin)
                    .header(header::HOST, host)
                    .body(Body::from("date=2024-01-15&notes=Leg%20day"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::SEE_OTHER,
            "Origin {origin} / Host {host} must reach the handler"
        );

        let workout_repo = WorkoutRepository::new(pool);
        let count = workout_repo.count_sessions_by_user(&user.id).await.unwrap();
        assert_eq!(count, 1);
    }
}

#[tokio::test]
async fn same_origin_post_succeeds() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie = common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workouts")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie)
                .header("sec-fetch-site", "same-origin")
                .body(Body::from("date=2024-01-15&notes=Leg%20day"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let workout_repo = WorkoutRepository::new(pool);
    let count = workout_repo.count_sessions_by_user(&user.id).await.unwrap();
    assert_eq!(count, 1);
}

/// A header-less (curl-shaped) POST passes as a non-browser client.
#[tokio::test]
async fn header_less_post_still_works() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie = common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workouts")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie)
                .body(Body::from("date=2024-01-15&notes=Leg%20day"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let workout_repo = WorkoutRepository::new(pool);
    let count = workout_repo.count_sessions_by_user(&user.id).await.unwrap();
    assert_eq!(count, 1);
}

/// Login CSRF: a cross-site `POST /auth/login` is rejected too.
#[tokio::test]
async fn login_csrf_is_blocked() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header("sec-fetch-site", "cross-site")
                .body(Body::from("username=admin&password=password123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
