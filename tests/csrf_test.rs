mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use liftlog::models::UserRole;
use liftlog::repositories::WorkoutRepository;
use tower::ServiceExt;

/// A cross-site POST carrying a valid session cookie is rejected before it can
/// mutate state.
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

    // The request never reached the handler, so no workout row was created.
    let workout_repo = WorkoutRepository::new(pool);
    let count = workout_repo.count_sessions_by_user(&user.id).await.unwrap();
    assert_eq!(count, 0);
}

/// A POST whose `Origin` host does not match the request `Host` is rejected via
/// the fallback path (no `Sec-Fetch-Site`).
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

/// A same-origin POST (as a real browser marks it) passes the guard and mutates
/// state normally.
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

/// The existing header-less harness POST (curl-shaped, no `Origin`/
/// `Sec-Fetch-Site`) still works — the guard treats it as a non-browser client.
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

/// Login-CSRF is covered statelessly: the pre-auth `POST /auth/login` is
/// rejected when reported cross-site.
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
