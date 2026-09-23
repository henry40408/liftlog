mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use liftlog::models::UserRole;
use liftlog::repositories::{SessionRepository, UserRepository, ValidateOutcome};
use tower::ServiceExt;

#[tokio::test]
async fn test_admin_can_access_users_page() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let session_cookie = common::create_session_cookie(&pool, &admin).await;
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

    assert!(body_str.contains("admin"));
}

#[tokio::test]
async fn test_user_can_access_users_page() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
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
}

#[tokio::test]
async fn test_user_cannot_access_new_user_page() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_admin_can_access_new_user_page() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let session_cookie = common::create_session_cookie(&pool, &admin).await;
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
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;

    let session_cookie = common::create_session_cookie(&pool, &admin).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/delete", user.id))
                .header(header::COOKIE, &cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("current_password=adminpass"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/users");

    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_id(&user.id).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_user_cannot_delete_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password", UserRole::User).await;

    let session_cookie = common::create_session_cookie(&pool, &user1).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/delete", user2.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_id(&user2.id).await.unwrap();
    assert!(found.is_some());
}

#[tokio::test]
async fn test_admin_cannot_self_delete() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;

    let session_cookie = common::create_session_cookie(&pool, &admin).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/delete", admin.id))
                .header(header::COOKIE, &cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("current_password=adminpass"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Cannot delete yourself.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_id(&admin.id).await.unwrap();
    assert!(found.is_some());
}

#[tokio::test]
async fn test_admin_can_promote_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;

    let session_cookie = common::create_session_cookie(&pool, &admin).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/promote", user.id))
                .header(header::COOKIE, &cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("current_password=adminpass"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/users");

    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_id(&user.id).await.unwrap().unwrap();
    assert_eq!(found.role, UserRole::Admin);
}

/// Promotion kills the user's sessions, so a stolen user token never becomes
/// an admin token. The promoting admin keeps theirs.
#[tokio::test]
async fn test_promote_destroys_the_promoted_users_sessions() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;

    let session_repo = SessionRepository::new(pool.clone());
    let victim_token = session_repo.create(&user.id).await.unwrap();
    let admin_session = common::create_session_cookie(&pool, &admin).await;
    let cookie_header = common::extract_cookie_header(&admin_session);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/promote", user.id))
                .header(header::COOKIE, &cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("current_password=adminpass"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    assert!(
        matches!(
            session_repo
                .validate_and_touch(&victim_token)
                .await
                .unwrap(),
            ValidateOutcome::Unknown
        ),
        "the promoted user's pre-existing session must be destroyed"
    );
    assert_eq!(
        session_repo.count_for_user(&admin.id).await.unwrap(),
        1,
        "the promoting admin's own session must survive"
    );
}

#[tokio::test]
async fn test_user_cannot_promote_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password", UserRole::User).await;

    let session_cookie = common::create_session_cookie(&pool, &user1).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/promote", user2.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_id(&user2.id).await.unwrap().unwrap();
    assert_eq!(found.role, UserRole::User);
}

#[tokio::test]
async fn test_admin_can_create_new_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;

    let session_cookie = common::create_session_cookie(&pool, &admin).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/new")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from(format!(
                    "username=newuser&password={}",
                    common::STRONG_PASSWORD
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/users");

    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_username("newuser").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().role, UserRole::User);
}

#[tokio::test]
async fn test_user_cannot_create_new_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;

    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/new")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from(format!(
                    "username=newuser&password={}",
                    common::STRONG_PASSWORD
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let user_repo = UserRepository::new(pool);
    let found = user_repo.find_by_username("newuser").await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_unauthenticated_cannot_access_users() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    // Otherwise the app redirects to setup.
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

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

/// An admin cookie alone cannot promote; the password is required.
#[tokio::test]
async fn test_promote_without_the_password_does_nothing() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &admin).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/promote", user.id))
                .header(header::COOKIE, &cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("current_password=notthepassword"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Re-rendered, not redirected: nothing happened.
    assert_eq!(response.status(), StatusCode::OK);

    let user_repo = UserRepository::new(pool);
    assert_eq!(
        user_repo.find_by_id(&user.id).await.unwrap().unwrap().role,
        UserRole::User,
        "a wrong confirmation password must leave the role untouched"
    );
}

#[tokio::test]
async fn test_delete_without_the_password_does_nothing() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &admin).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/delete", user.id))
                .header(header::COOKIE, &cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("current_password=notthepassword"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let user_repo = UserRepository::new(pool);
    assert!(
        user_repo.find_by_id(&user.id).await.unwrap().is_some(),
        "a wrong confirmation password must leave the account intact"
    );
}

/// The confirmation page names the target and the consequence.
#[tokio::test]
async fn test_delete_confirmation_page_names_the_target() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let user = common::create_test_user(&pool, "victimuser", "password", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &admin).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/users/{}/delete", user.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("victimuser"), "should name the target");
    assert!(
        html.contains("cannot be undone"),
        "should state the consequence"
    );
    assert!(
        html.contains("name=\"current_password\""),
        "should ask for the password"
    );
}

/// A non-admin can't see the confirmation page, which would reveal the user.
#[tokio::test]
async fn test_non_admin_cannot_open_the_confirmation_page() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &user1).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/users/{}/delete", user2.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// Re-auth shares the password-change throttle, so it is no guessing oracle.
#[tokio::test]
async fn test_reauth_is_throttled() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_password_change_limit(
        pool.clone(),
        2,
        std::time::Duration::from_secs(60),
    );

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let user = common::create_test_user(&pool, "regularuser", "password", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &admin).await);

    let guess = |cookie: &str| {
        Request::builder()
            .method("POST")
            .uri(format!("/users/{}/promote", user.id))
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("current_password=wrongguess"))
            .unwrap()
    };

    for _ in 0..2 {
        let response = test_app
            .router
            .clone()
            .oneshot(guess(&cookie_header))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = test_app
        .router
        .oneshot(guess(&cookie_header))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

/// The promote page says it grants admin, not just asks for a password.
#[tokio::test]
async fn test_promote_confirmation_page_names_the_target_and_the_grant() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let user = common::create_test_user(&pool, "candidate", "password", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &admin).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/users/{}/promote", user.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("candidate"), "should name the target");
    assert!(
        html.contains("administrative access"),
        "should say what is being granted"
    );
    assert!(
        html.contains("name=\"current_password\""),
        "should ask for the password"
    );
}

/// The self-delete guard fires on the confirmation page too, not only the POST.
#[tokio::test]
async fn test_delete_confirmation_page_refuses_self() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &admin).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/users/{}/delete", admin.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_confirmation_page_404s_for_an_unknown_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &admin).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/users/no-such-id/promote")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
