mod common;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use liftlog::models::UserRole;
use liftlog::repositories::WorkoutRepository;
use tower::ServiceExt;

#[tokio::test]
async fn test_share_workout_success() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        Some("Test workout"),
    )
    .await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/share", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to workout page
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .contains(&workout.id));

    // Verify share_token was set
    let workout_repo = WorkoutRepository::new(pool);
    let updated = workout_repo
        .find_session_by_id(&workout.id)
        .await
        .unwrap()
        .unwrap();
    assert!(updated.share_token.is_some());
}

#[tokio::test]
async fn test_view_shared_workout_public() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        Some("Shared workout test"),
    )
    .await;
    common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, 100.0, Some(8)).await;

    // Share the workout
    let workout_repo = WorkoutRepository::new(pool.clone());
    let share_token = workout_repo
        .set_share_token(&workout.id, &user.id)
        .await
        .unwrap();

    // View shared workout without auth (new app instance to avoid cookies)
    let app = common::create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(&format!("/shared/{}", share_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Verify content is shown
    assert!(body_str.contains("2024-01-15") || body_str.contains("Shared workout test"));
    assert!(body_str.contains("Bench Press"));
    assert!(body_str.contains("testuser"));
}

#[tokio::test]
async fn test_view_shared_invalid_token_returns_404() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/shared/invalid-token-12345")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_revoke_share_success() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;

    // First share the workout
    let workout_repo = WorkoutRepository::new(pool.clone());
    let share_token = workout_repo
        .set_share_token(&workout.id, &user.id)
        .await
        .unwrap();

    // Then revoke it
    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/revoke-share", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    // Verify share_token was revoked
    let updated = workout_repo
        .find_session_by_id(&workout.id)
        .await
        .unwrap()
        .unwrap();
    assert!(updated.share_token.is_none());

    // Verify the old token no longer works
    let app = common::create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(&format!("/shared/{}", share_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_reshare_after_revoke_generates_new_token() {
    let pool = common::setup_test_db();

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;

    let workout_repo = WorkoutRepository::new(pool.clone());

    // Share
    let token1 = workout_repo
        .set_share_token(&workout.id, &user.id)
        .await
        .unwrap();

    // Revoke
    workout_repo
        .revoke_share_token(&workout.id, &user.id)
        .await
        .unwrap();

    // Share again
    let token2 = workout_repo
        .set_share_token(&workout.id, &user.id)
        .await
        .unwrap();

    // Tokens should be different
    assert_ne!(token1, token2);

    // Old token should not work
    let app = common::create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(&format!("/shared/{}", token1))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // New token should work
    let app2 = common::create_test_app(pool.clone());
    let response2 = app2
        .oneshot(
            Request::builder()
                .uri(&format!("/shared/{}", token2))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response2.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_cannot_share_others_workout() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    // user2 creates workout
    let workout = common::create_test_workout(
        &pool,
        &user2.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;

    // user1 tries to share it
    let session_cookie = common::create_session_cookie(&pool, &user1).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/share", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Verify workout was NOT shared
    let workout_repo = WorkoutRepository::new(pool);
    let found = workout_repo
        .find_session_by_id(&workout.id)
        .await
        .unwrap()
        .unwrap();
    assert!(found.share_token.is_none());
}

#[tokio::test]
async fn test_share_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/share", workout.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to login
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_revoke_share_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/revoke-share", workout.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to login
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_cannot_revoke_others_share() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    // user2 creates and shares workout
    let workout = common::create_test_workout(
        &pool,
        &user2.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let workout_repo = WorkoutRepository::new(pool.clone());
    let share_token = workout_repo
        .set_share_token(&workout.id, &user2.id)
        .await
        .unwrap();

    // user1 tries to revoke it
    let session_cookie = common::create_session_cookie(&pool, &user1).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/revoke-share", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Verify workout share was NOT revoked
    let found = workout_repo
        .find_session_by_id(&workout.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.share_token, Some(share_token));
}

#[tokio::test]
async fn test_show_workout_displays_share_button() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(&format!("/workouts/{}", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Should show share button
    assert!(body_str.contains("[Share]"));
    // Should not show revoke button or share link
    assert!(!body_str.contains("[Revoke Share]"));
    assert!(!body_str.contains("Share link:"));
}

#[tokio::test]
async fn test_show_workout_displays_share_link_and_revoke() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;

    // Share the workout
    let workout_repo = WorkoutRepository::new(pool.clone());
    let share_token = workout_repo
        .set_share_token(&workout.id, &user.id)
        .await
        .unwrap();

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(&format!("/workouts/{}", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Should show revoke button and share link
    assert!(body_str.contains("[Revoke Share]"));
    assert!(body_str.contains("Share link:"));
    assert!(body_str.contains(&format!("/shared/{}", share_token)));
    // Should not show share button
    assert!(!body_str.contains(">[Share]<"));
}
