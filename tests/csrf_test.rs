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

/// `Sec-Fetch-Site: same-site` is a *different* origin that `SameSite=Lax`
/// still hands the session cookie — a sibling subdomain, or another port on the
/// same host. The guard used to allow it; it must not.
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

/// Without `Sec-Fetch-Site` — an old browser, or a plain-HTTP LAN origin, which
/// is not potentially-trustworthy and so never receives fetch metadata — the
/// `Origin`/`Host` comparison is the *only* check running, and it compares the
/// full authority. The port is the point: cookies ignore ports, so a page on
/// another port of the same host would otherwise post with the victim's own
/// session attached.
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

/// The two shapes the fallback must keep working: a bare LAN install on a
/// non-default port, and a TLS-terminating proxy whose forwarded `Host` carries
/// no scheme and no port because the browser used the default one.
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
