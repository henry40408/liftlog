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
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
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

// Session edit tests

#[tokio::test]
async fn test_edit_workout_page_renders() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

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
                .uri(&format!("/workouts/{}/edit", workout.id))
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
async fn test_update_workout_success() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

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
                .uri(&format!("/workouts/{}", workout.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("date=2024-01-20&notes=Updated%20notes"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    // Verify workout was updated
    let updated = workout_repo.find_session_by_id(&workout.id).await.unwrap().unwrap();
    assert_eq!(updated.date, chrono::NaiveDate::from_ymd_opt(2024, 1, 20).unwrap());
    assert_eq!(updated.notes, Some("Updated notes".to_string()));
}

#[tokio::test]
async fn test_cannot_edit_others_workout_page() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    let workout_repo = WorkoutRepository::new(pool.clone());
    let workout = workout_repo
        .create_session(
            &user2.id,
            chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            None,
        )
        .await
        .unwrap();

    let session_cookie = common::create_session_cookie(&user1, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(&format!("/workouts/{}/edit", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// Log management tests

#[tokio::test]
async fn test_add_log_success() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
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
                .method("POST")
                .uri(&format!("/workouts/{}/logs", workout.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from(format!(
                    "exercise_id={}&reps=10&weight=100&rpe=8",
                    exercise.id
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .contains(&workout.id));

    // Verify log was created
    let workout_repo = WorkoutRepository::new(pool);
    let logs = workout_repo
        .find_logs_by_session_with_pr(&workout.id, &user.id)
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].reps, 10);
    assert_eq!(logs[0].weight, 100.0);
}

#[tokio::test]
async fn test_add_log_requires_ownership() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    let exercise = common::create_test_exercise(&pool, &user1.id, "Bench Press", "chest").await;
    let workout = common::create_test_workout(
        &pool,
        &user2.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;

    let session_cookie = common::create_session_cookie(&user1, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/logs", workout.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from(format!(
                    "exercise_id={}&reps=10&weight=100",
                    exercise.id
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_log_success() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let log = common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, 100.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/logs/{}/delete", workout.id, log.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    // Verify log was deleted
    let workout_repo = WorkoutRepository::new(pool);
    let logs = workout_repo
        .find_logs_by_session_with_pr(&workout.id, &user.id)
        .await
        .unwrap();
    assert_eq!(logs.len(), 0);
}

#[tokio::test]
async fn test_delete_log_requires_ownership() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    let exercise = common::create_test_exercise(&pool, &user2.id, "Bench Press", "chest").await;
    let workout = common::create_test_workout(
        &pool,
        &user2.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let log = common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, 100.0, None).await;

    let session_cookie = common::create_session_cookie(&user1, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/logs/{}/delete", workout.id, log.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Verify log was NOT deleted
    let workout_repo = WorkoutRepository::new(pool);
    let found = workout_repo.find_log_by_id(&log.id).await.unwrap();
    assert!(found.is_some());
}

// Log editing tests

#[tokio::test]
async fn test_edit_log_page_renders() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let log = common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, 100.0, Some(8)).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(&format!("/workouts/{}/logs/{}/edit", workout.id, log.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    assert!(body_str.contains("Bench Press"));
    assert!(body_str.contains("100") || body_str.contains("10"));
}

#[tokio::test]
async fn test_update_log_success() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let log = common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, 100.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/logs/{}", workout.id, log.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("reps=12&weight=110&rpe=9"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    // Verify log was updated
    let workout_repo = WorkoutRepository::new(pool);
    let updated = workout_repo.find_log_by_id(&log.id).await.unwrap().unwrap();
    assert_eq!(updated.reps, 12);
    assert_eq!(updated.weight, 110.0);
    assert_eq!(updated.rpe, Some(9));
}

#[tokio::test]
async fn test_update_log_requires_ownership() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    let exercise = common::create_test_exercise(&pool, &user2.id, "Bench Press", "chest").await;
    let workout = common::create_test_workout(
        &pool,
        &user2.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let log = common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, 100.0, None).await;

    let session_cookie = common::create_session_cookie(&user1, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/workouts/{}/logs/{}", workout.id, log.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("reps=12&weight=110&rpe=9"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Verify log was NOT updated
    let workout_repo = WorkoutRepository::new(pool);
    let found = workout_repo.find_log_by_id(&log.id).await.unwrap().unwrap();
    assert_eq!(found.reps, 10);
    assert_eq!(found.weight, 100.0);
}

// Pagination tests

#[tokio::test]
async fn test_workouts_list_pagination_page_2() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    // Create 15 workouts (more than one page of 10)
    for i in 1..=15 {
        common::create_test_workout(
            &pool,
            &user.id,
            chrono::NaiveDate::from_ymd_opt(2024, 1, i as u32).unwrap(),
            Some(&format!("Workout {}", i)),
        )
        .await;
    }

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/workouts?page=2")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Page 2 should have the older workouts (workouts 1-5 since ordered by date DESC)
    // First page has workouts 15-6
    assert!(body_str.contains("2024-01-01") || body_str.contains("2024-01-05"));
}
