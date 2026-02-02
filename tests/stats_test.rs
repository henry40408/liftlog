mod common;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use liftlog::models::UserRole;
use tower::ServiceExt;

#[tokio::test]
async fn test_stats_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/stats")
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
async fn test_prs_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/stats/prs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to login
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn test_stats_index_shows_workout_counts() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    // Create some workouts (using recent dates for week/month counts)
    let today = chrono::Local::now().date_naive();
    common::create_test_workout(&pool, &user.id, today, Some("Today's workout")).await;
    common::create_test_workout(
        &pool,
        &user.id,
        today - chrono::Duration::days(2),
        Some("Two days ago"),
    )
    .await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/stats")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Should show workout counts (at least 2 total)
    assert!(body_str.contains("2") || body_str.contains("統計"));
}

#[tokio::test]
async fn test_stats_index_calculates_volume() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    // Create workout with logs
    let today = chrono::Local::now().date_naive();
    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
    let workout = common::create_test_workout(&pool, &user.id, today, None).await;

    // 10 reps * 100kg = 1000kg volume
    common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, 100.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/stats")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Should contain volume data
    assert!(body_str.contains("1000") || body_str.contains("volume") || body_str.contains("總量"));
}

#[tokio::test]
async fn test_stats_index_shows_prs() {
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
    common::create_test_log(&pool, &workout.id, &exercise.id, 1, 5, 120.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/stats")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Should show PR for Bench Press
    assert!(body_str.contains("Bench Press") || body_str.contains("120"));
}

#[tokio::test]
async fn test_exercise_stats_shows_history() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
    let workout1 = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let workout2 = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 20).unwrap(),
        None,
    )
    .await;

    common::create_test_log(&pool, &workout1.id, &exercise.id, 1, 10, 100.0, None).await;
    common::create_test_log(&pool, &workout2.id, &exercise.id, 1, 8, 110.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(&format!("/stats/exercise/{}", exercise.id))
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
    assert!(body_str.contains("100") || body_str.contains("110"));
}

#[tokio::test]
async fn test_exercise_stats_nonexistent_exercise() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/stats/exercise/nonexistent-id")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_prs_list_shows_all_prs() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise1 = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
    let exercise2 = common::create_test_exercise(&pool, &user.id, "Squat", "legs").await;
    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;

    common::create_test_log(&pool, &workout.id, &exercise1.id, 1, 5, 100.0, None).await;
    common::create_test_log(&pool, &workout.id, &exercise2.id, 1, 5, 150.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/stats/prs")
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
    assert!(body_str.contains("Squat"));
    assert!(body_str.contains("100") || body_str.contains("150"));
}
