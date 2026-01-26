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
async fn test_workouts_list_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/workouts")
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
async fn test_new_workout_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/workouts/new")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to login
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn test_create_workout_authenticated() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create a test user
    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workouts")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("date=2024-01-15&notes=Leg%20day"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to the workout page
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("/workouts/"));

    // Verify workout was created in database
    let workout_repo = WorkoutRepository::new(pool);
    let count = workout_repo.count_sessions_by_user(&user.id).await.unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_workout_list_shows_user_workouts() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create a test user and some workouts
    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    // Create workouts directly via repository
    let workout_repo = WorkoutRepository::new(pool.clone());
    workout_repo
        .create_session(
            &user.id,
            chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            Some("Chest day"),
        )
        .await
        .unwrap();
    workout_repo
        .create_session(
            &user.id,
            chrono::NaiveDate::from_ymd_opt(2024, 1, 16).unwrap(),
            Some("Back day"),
        )
        .await
        .unwrap();

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/workouts")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Verify workouts are shown
    assert!(body_str.contains("Chest day") || body_str.contains("2024-01-15"));
    assert!(body_str.contains("Back day") || body_str.contains("2024-01-16"));
}

#[tokio::test]
async fn test_workout_list_only_shows_own_workouts() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create two users
    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    // Create workouts for both users
    let workout_repo = WorkoutRepository::new(pool.clone());
    workout_repo
        .create_session(
            &user1.id,
            chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            Some("User1 workout"),
        )
        .await
        .unwrap();
    workout_repo
        .create_session(
            &user2.id,
            chrono::NaiveDate::from_ymd_opt(2024, 1, 16).unwrap(),
            Some("User2 workout"),
        )
        .await
        .unwrap();

    // Login as user1
    let session_cookie = common::create_session_cookie(&user1, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/workouts")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // User1 should see their workout but not User2's
    assert!(body_str.contains("User1 workout") || body_str.contains("2024-01-15"));
    assert!(!body_str.contains("User2 workout"));
}

#[tokio::test]
async fn test_delete_workout() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create a test user and a workout
    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let workout_repo = WorkoutRepository::new(pool.clone());
    let workout = workout_repo
        .create_session(
            &user.id,
            chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            None,
        )
        .await
        .unwrap();

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/delete", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to workouts list
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/workouts");

    // Verify workout was deleted
    let count = workout_repo.count_sessions_by_user(&user.id).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_cannot_delete_others_workout() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create two users
    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    // Create a workout for user2
    let workout_repo = WorkoutRepository::new(pool.clone());
    let workout = workout_repo
        .create_session(
            &user2.id,
            chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            None,
        )
        .await
        .unwrap();

    // Login as user1 and try to delete user2's workout
    let session_cookie = common::create_session_cookie(&user1, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/delete", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should still redirect (delete returns success even if no rows affected)
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    // But the workout should still exist
    let found = workout_repo.find_session_by_id(&workout.id).await.unwrap();
    assert!(found.is_some());
}

#[tokio::test]
async fn test_view_workout_details() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create a test user and a workout
    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let workout_repo = WorkoutRepository::new(pool.clone());
    let workout = workout_repo
        .create_session(
            &user.id,
            chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            Some("Test workout"),
        )
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

    assert!(body_str.contains("2024-01-15") || body_str.contains("Test workout"));
}

#[tokio::test]
async fn test_cannot_view_others_workout() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    // Create two users
    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    // Create a workout for user2
    let workout_repo = WorkoutRepository::new(pool.clone());
    let workout = workout_repo
        .create_session(
            &user2.id,
            chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            None,
        )
        .await
        .unwrap();

    // Login as user1 and try to view user2's workout
    let session_cookie = common::create_session_cookie(&user1, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

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

    // Should return 404 (not found - for security we don't reveal existence)
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
