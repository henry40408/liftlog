use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use liftlog::models::UserRole;
use liftlog::repositories::UserRepository;
use tower::ServiceExt;

mod common;

fn setup_request(token: Option<&str>) -> Request<Body> {
    let token_field = token.map_or(String::new(), |t| format!("&setup_token={t}"));
    let body = format!(
        "username=admin&password={}{token_field}",
        common::STRONG_PASSWORD
    );
    Request::builder()
        .method("POST")
        .uri("/auth/setup")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap()
}

async fn user_count(pool: &liftlog::db::DbPool) -> i64 {
    UserRepository::new(pool.clone()).count().await.unwrap()
}

#[tokio::test]
async fn setup_without_token_is_forbidden() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool.clone());

    let response = app.oneshot(setup_request(None)).await.unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(user_count(&pool).await, 0);
}

#[tokio::test]
async fn setup_with_wrong_token_is_forbidden() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool.clone());

    let response = app
        .oneshot(setup_request(Some("ZZZZZ-ZZZZZ")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(user_count(&pool).await, 0);
}

#[tokio::test]
async fn setup_with_correct_token_creates_admin() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool.clone());

    let response = app
        .oneshot(setup_request(Some(common::TEST_SETUP_TOKEN)))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get(header::LOCATION).unwrap(), "/");
    assert_eq!(user_count(&pool).await, 1);
}

#[tokio::test]
async fn setup_token_ignores_case_dash_and_whitespace() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool.clone());

    // "abcde fghjk" url-encoded: lowercase, space instead of the dash.
    let response = app
        .oneshot(setup_request(Some("abcde%20fghjk")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(user_count(&pool).await, 1);
}

#[tokio::test]
async fn token_is_single_use() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());
    let token = test_app.setup_token.clone();

    let response = test_app
        .router
        .clone()
        .oneshot(setup_request(Some(common::TEST_SETUP_TOKEN)))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(token.current(), None, "token must be cleared after setup");
    assert!(!token.verify(common::TEST_SETUP_TOKEN));
}

#[tokio::test]
async fn setup_page_has_token_field() {
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

    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    assert!(String::from_utf8_lossy(&body).contains("name=\"setup_token\""));
}

#[tokio::test]
async fn setup_is_throttled_per_ip_and_attempts_are_not_refunded() {
    let pool = common::setup_test_db();
    let app = common::create_test_app_with_setup_limit(pool.clone(), 2).router;

    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(setup_request(Some("ZZZZZ-ZZZZZ")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // Even the right token is refused once the budget is spent.
    let response = app
        .oneshot(setup_request(Some(common::TEST_SETUP_TOKEN)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(user_count(&pool).await, 0);
}

#[tokio::test]
async fn existing_users_redirect_before_the_token_is_checked() {
    let pool = common::setup_test_db();
    common::create_test_user(&pool, "existing", "password", UserRole::User).await;
    let app = common::create_test_app(pool);

    let response = app.oneshot(setup_request(None)).await.unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "/auth/login"
    );
}

#[tokio::test]
async fn right_token_with_bad_password_is_refunded_but_wrong_token_is_not() {
    let pool = common::setup_test_db();
    let app = common::create_test_app_with_setup_limit(pool.clone(), 2).router;
    let weak_password = |token: &str| {
        Request::builder()
            .method("POST")
            .uri("/auth/setup")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!(
                "username=admin&password=abc&setup_token={token}"
            )))
            .unwrap()
    };

    // More failures than the budget of 2: each right-token attempt is refunded.
    for _ in 0..4 {
        let response = app
            .clone()
            .oneshot(weak_password(common::TEST_SETUP_TOKEN))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(weak_password("ZZZZZ-ZZZZZ"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    let response = app.oneshot(weak_password("ZZZZZ-ZZZZZ")).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}
