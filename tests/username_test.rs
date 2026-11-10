use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use liftlog::models::UserRole;
use liftlog::repositories::UserRepository;
use tower::ServiceExt;

mod common;

async fn post_form(
    router: axum::Router,
    uri: &str,
    cookie: Option<&str>,
    body: String,
) -> (StatusCode, String) {
    let mut request = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    let response = router
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn setup_body(username: &str) -> String {
    format!(
        "username={username}&password={}&setup_token={}",
        common::STRONG_PASSWORD,
        common::TEST_SETUP_TOKEN
    )
}

async fn admin_cookie(pool: &liftlog::db::DbPool) -> String {
    let admin = common::create_test_user(pool, "admin", "adminpass", UserRole::Admin).await;
    let session_cookie = common::create_session_cookie(pool, &admin).await;
    common::extract_cookie_header(&session_cookie)
}

fn new_user_body(username: &str) -> String {
    format!("username={username}&password={}", common::STRONG_PASSWORD)
}

#[tokio::test]
async fn setup_stores_the_trimmed_username() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool.clone());

    let (status, _) = post_form(app, "/auth/setup", None, setup_body("++lifter++")).await;
    // "+" decodes to a space.
    assert_eq!(status, StatusCode::SEE_OTHER);

    let repo = UserRepository::new(pool);
    assert!(repo.find_by_username("lifter").await.unwrap().is_some());
    assert!(repo.find_by_username(" lifter ").await.unwrap().is_none());
}

#[tokio::test]
async fn setup_rejects_overlong_username() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool.clone());

    let (status, html) = post_form(app, "/auth/setup", None, setup_body(&"a".repeat(65))).await;

    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("at most 64"));
    assert_eq!(UserRepository::new(pool).count().await.unwrap(), 0);
}

#[tokio::test]
async fn setup_rejects_control_characters_in_username() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool.clone());

    let (status, html) = post_form(app, "/auth/setup", None, setup_body("a%0Ab")).await;

    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("control characters"));
    assert_eq!(UserRepository::new(pool).count().await.unwrap(), 0);
}

#[tokio::test]
async fn new_user_stores_the_trimmed_username() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool.clone());
    let cookie = admin_cookie(&pool).await;

    let (status, _) = post_form(app, "/users/new", Some(&cookie), new_user_body("++bob++")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let repo = UserRepository::new(pool);
    assert!(repo.find_by_username("bob").await.unwrap().is_some());
}

#[tokio::test]
async fn new_user_trimmed_duplicate_is_rejected() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool.clone());
    let cookie = admin_cookie(&pool).await;

    let (_, html) = post_form(app, "/users/new", Some(&cookie), new_user_body("++admin++")).await;

    assert!(html.contains("Username already exists"));
}

#[tokio::test]
async fn new_user_rejects_overlong_and_control_usernames() {
    let pool = common::setup_test_db();
    let cookie = admin_cookie(&pool).await;

    for (name, expected) in [
        ("a".repeat(65), "at most 64"),
        ("a%09b".to_string(), "control characters"),
    ] {
        let app = common::create_test_app(pool.clone());
        let (status, html) =
            post_form(app, "/users/new", Some(&cookie), new_user_body(&name)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains(expected), "{name}: {html}");
    }
    assert_eq!(UserRepository::new(pool).count().await.unwrap(), 1);
}
