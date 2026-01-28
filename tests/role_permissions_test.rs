mod common;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use liftlog::models::UserRole;
use liftlog::repositories::UserRepository;
use tower::ServiceExt;

#[tokio::test]
async fn test_admin_can_access_users_page() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create an admin user
    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let session_cookie = common::create_session_cookie(&admin, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/users")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Should show the users list with the admin user
    assert!(body_str.contains("admin"));
}

#[tokio::test]
async fn test_user_can_access_users_page() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create a regular user
    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/users")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Users list is accessible to all logged in users (they can see the list)
    // but admin-only actions are restricted
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_user_cannot_access_new_user_page() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create a regular user
    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/users/new")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Regular users should get 403 Forbidden
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_admin_can_access_new_user_page() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create an admin user
    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let session_cookie = common::create_session_cookie(&admin, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/users/new")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_admin_can_delete_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create an admin and a regular user
    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;

    let session_cookie = common::create_session_cookie(&admin, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/users/{}/delete", user.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to users list
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/users");

    // Verify user was deleted
    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_id(&user.id).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_user_cannot_delete_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create two regular users
    let user1 = common::create_test_user(&pool, "user1", "password", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password", UserRole::User).await;

    let session_cookie = common::create_session_cookie(&user1, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/users/{}/delete", user2.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should get 403 Forbidden
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // User should still exist
    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_id(&user2.id).await.unwrap();
    assert!(found.is_some());
}

#[tokio::test]
async fn test_admin_cannot_self_delete() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create an admin
    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;

    let session_cookie = common::create_session_cookie(&admin, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/users/{}/delete", admin.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should get 400 Bad Request (cannot delete yourself)
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Admin should still exist
    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_id(&admin.id).await.unwrap();
    assert!(found.is_some());
}

#[tokio::test]
async fn test_admin_can_promote_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create an admin and a regular user
    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;

    let session_cookie = common::create_session_cookie(&admin, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/users/{}/promote", user.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to users list
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/users");

    // Verify user was promoted to admin
    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_id(&user.id).await.unwrap().unwrap();
    assert_eq!(found.role, UserRole::Admin);
}

#[tokio::test]
async fn test_user_cannot_promote_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create two regular users
    let user1 = common::create_test_user(&pool, "user1", "password", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password", UserRole::User).await;

    let session_cookie = common::create_session_cookie(&user1, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/users/{}/promote", user2.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should get 403 Forbidden
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // User2 should still be a regular user
    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_id(&user2.id).await.unwrap().unwrap();
    assert_eq!(found.role, UserRole::User);
}

#[tokio::test]
async fn test_admin_can_create_new_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create an admin
    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;

    let session_cookie = common::create_session_cookie(&admin, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/new")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("username=newuser&password=password123"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to users list
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/users");

    // Verify new user was created
    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_username("newuser").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().role, UserRole::User);
}

#[tokio::test]
async fn test_user_cannot_create_new_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create a regular user
    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;

    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/new")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("username=newuser&password=password123"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should get 403 Forbidden
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // User should not be created
    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_username("newuser").await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_unauthenticated_cannot_access_users() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create a user so the app doesn't redirect to setup
    common::create_test_user(&pool, "existing", "password", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to login
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}
