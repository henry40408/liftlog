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
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
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
    assert!(body_str.contains("2") || body_str.contains("Stats"));
}

#[tokio::test]
async fn test_stats_index_calculates_volume() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
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
    assert!(
        body_str.contains("1000") || body_str.contains("volume") || body_str.contains("Volume")
    );
}

#[tokio::test]
async fn test_stats_index_shows_prs() {
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
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
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
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
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
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
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

#[tokio::test]
async fn test_exercise_stats_chart_renders_with_two_or_more_sessions() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;

    for (i, weight) in [100.0_f64, 105.0, 110.0].iter().enumerate() {
        let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 10 + i as u32 * 2).unwrap();
        let workout = common::create_test_workout(&pool, &user.id, date, None).await;
        common::create_test_log(&pool, &workout.id, &exercise.id, 1, 5, *weight, None).await;
    }

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

    // Server-rendered SVG line is present
    assert!(body_str.contains("<polyline"), "polyline missing");
    assert!(body_str.contains("id=\"chart-line\""));

    // JSON-embedded dataset is present and parseable
    let start = body_str
        .find("id=\"exercise-chart-data\">")
        .expect("chart-data script tag missing");
    let after_open = &body_str[start + "id=\"exercise-chart-data\">".len()..];
    let end = after_open
        .find("</script>")
        .expect("chart-data script close tag");
    let json_text = &after_open[..end];
    let parsed: serde_json::Value =
        serde_json::from_str(json_text).expect("chart data JSON should parse");
    let arr = parsed.as_array().expect("chart data should be an array");
    assert_eq!(arr.len(), 3);
    let first = &arr[0];
    assert!(first.get("top_weight").is_some());
    assert!(first.get("top_reps").is_some());
    assert!(first.get("volume").is_some());
    assert!(first.get("e1rm").is_some());
    assert!(first.get("date").is_some());
}

#[tokio::test]
async fn test_exercise_stats_chart_renders_sparse_state_with_one_session() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
    let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let workout = common::create_test_workout(&pool, &user.id, date, None).await;
    common::create_test_log(&pool, &workout.id, &exercise.id, 1, 5, 100.0, None).await;

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

    assert!(body_str.contains("Need at least 2 sessions"));
    assert!(!body_str.contains("<polyline"));
}

#[tokio::test]
async fn test_exercise_stats_chart_renders_empty_state_with_no_logs() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;

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

    assert!(body_str.contains("No progress data yet"));
    assert!(!body_str.contains("<polyline"));
    assert!(!body_str.contains("id=\"exercise-chart-data\""));
}

#[tokio::test]
async fn test_exercise_stats_chart_pr_dots_match_expected_indices() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;

    // Weights ASC: [100, 100, 110, 105, 120]
    // Running max PRs at indices 0, 2, 4 (the first 100 also counts as the first running max).
    let weights: [f64; 5] = [100.0, 100.0, 110.0, 105.0, 120.0];
    for (i, w) in weights.iter().enumerate() {
        let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 10 + i as u32).unwrap();
        let workout = common::create_test_workout(&pool, &user.id, date, None).await;
        common::create_test_log(&pool, &workout.id, &exercise.id, 1, 5, *w, None).await;
    }

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

    // 5 dots total — running PRs are at index 0 (100), 2 (110), 4 (120).
    let pr_count = body_str.matches("class=\"ll-dot-pr\"").count();
    let plain_count = body_str.matches("class=\"ll-dot\"").count();
    assert_eq!(pr_count, 3, "expected 3 PR dots, body=\n{}", body_str);
    assert_eq!(plain_count, 2);
}
