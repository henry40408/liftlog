mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use liftlog::models::{UserRole, recent_pr_window_start};
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

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn test_create_workout_authenticated() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
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

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.starts_with("/workouts/"));

    let workout_repo = WorkoutRepository::new(pool);
    let count = workout_repo.count_sessions_by_user(&user.id).await.unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_workout_list_shows_user_workouts() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

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

    assert!(body_str.contains("Chest day") || body_str.contains("2024-01-15"));
    assert!(body_str.contains("Back day") || body_str.contains("2024-01-16"));
}

#[tokio::test]
async fn test_workout_list_only_shows_own_workouts() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

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

    let session_cookie = common::create_session_cookie(&pool, &user1).await;
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
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
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
                .uri(format!("/workouts/{}/delete", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/workouts");

    let count = workout_repo.count_sessions_by_user(&user.id).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_cannot_delete_others_workout() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

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

    let session_cookie = common::create_session_cookie(&pool, &user1).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/delete", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should still redirect (delete returns success even if no rows affected)
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let found = workout_repo.find_session_by_id(&workout.id).await.unwrap();
    assert!(found.is_some());
}

#[tokio::test]
async fn test_view_workout_details() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
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
                .uri(format!("/workouts/{}", workout.id))
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
    let test_app = common::create_test_app_with_session(pool.clone());

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

    let session_cookie = common::create_session_cookie(&pool, &user1).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/workouts/{}", workout.id))
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
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
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
                .uri(format!("/workouts/{}/edit", workout.id))
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
async fn test_workout_page_badges_a_one_month_pr_below_the_all_time_best() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;

    // The all-time best, logged well outside the 1-month window.
    let old_workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let old_log =
        common::create_test_log(&pool, &old_workout.id, &exercise.id, 1, 3, 140.0, None).await;
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE workout_logs SET created_at = ? WHERE id = ?",
            rusqlite::params![chrono::Utc::now() - chrono::Duration::days(90), old_log.id],
        )
        .unwrap();
    }

    // Today's session: lighter than the all-time PR, but the best this month.
    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
        None,
    )
    .await;
    common::create_test_log(&pool, &workout.id, &exercise.id, 1, 8, 110.0, None).await;

    let response = test_app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/workouts/{}", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    assert!(
        body_str.contains("class=\"pr-badge pr-badge-recent\""),
        "expected a 1-month PR badge, body=\n{body_str}"
    );
    assert!(
        body_str.contains("PR 1M"),
        "expected the 1M badge label, body=\n{body_str}"
    );

    // The older workout keeps the all-time badge and gains no 1-month one.
    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/workouts/{}", old_workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    assert!(
        !body_str.contains("class=\"pr-badge pr-badge-recent\""),
        "old workout should carry no 1-month badge, body=\n{body_str}"
    );
    assert!(
        body_str.contains("class=\"pr-badge\""),
        "old workout should still show the all-time PR badge, body=\n{body_str}"
    );
}

#[tokio::test]
async fn test_update_workout_success() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
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
                .uri(format!("/workouts/{}", workout.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("date=2024-01-20&notes=Updated%20notes"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let updated = workout_repo
        .find_session_by_id(&workout.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.date,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 20).unwrap()
    );
    assert_eq!(updated.notes, Some("Updated notes".to_string()));
}

#[tokio::test]
async fn test_cannot_edit_others_workout_page() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

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

    let session_cookie = common::create_session_cookie(&pool, &user1).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/workouts/{}/edit", workout.id))
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
#[allow(clippy::float_cmp, reason = "exact-value test assertion")]
async fn test_add_log_success() {
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

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/logs", workout.id))
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
    assert!(
        response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap()
            .contains(&workout.id)
    );

    let workout_repo = WorkoutRepository::new(pool);
    let logs = workout_repo
        .find_logs_by_session_with_pr(&workout.id, &user.id, recent_pr_window_start())
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].reps, 10);
    assert_eq!(logs[0].weight, 100.0);
}

#[tokio::test]
async fn test_add_log_rejects_exercise_owned_by_another_user() {
    // Owning the workout session does not entitle the caller to reference an
    // exercise belonging to somebody else: `exercise_id` comes straight from the
    // form body, so it has to be authorized independently of the session.
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let attacker = common::create_test_user(&pool, "attacker", "password123", UserRole::User).await;
    let victim = common::create_test_user(&pool, "victim", "password123", UserRole::User).await;

    let session_cookie = common::create_session_cookie(&pool, &attacker).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let victim_exercise =
        common::create_test_exercise(&pool, &victim.id, "Victim Squat", "legs").await;
    let workout = common::create_test_workout(
        &pool,
        &attacker.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/logs", workout.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from(format!(
                    "exercise_id={}&reps=10&weight=100&rpe=8",
                    victim_exercise.id
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // The write must not have happened at all, not merely been reported as denied.
    let workout_repo = WorkoutRepository::new(pool);
    let logs = workout_repo
        .find_logs_by_session_with_pr(&workout.id, &attacker.id, recent_pr_window_start())
        .await
        .unwrap();
    assert!(
        logs.is_empty(),
        "no log should have been created, got {} log(s)",
        logs.len()
    );
}

#[tokio::test]
#[allow(clippy::float_cmp, reason = "exact-value test assertion")]
async fn test_add_log_accepts_fractional_weight() {
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

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/logs", workout.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from(format!(
                    "exercise_id={}&reps=10&weight=21.25&rpe=8",
                    exercise.id
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let workout_repo = WorkoutRepository::new(pool);
    let logs = workout_repo
        .find_logs_by_session_with_pr(&workout.id, &user.id, recent_pr_window_start())
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].weight, 21.25);
}

#[tokio::test]
async fn test_add_log_requires_ownership() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

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

    let session_cookie = common::create_session_cookie(&pool, &user1).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/logs", workout.id))
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
    let log = common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, 100.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/logs/{}/delete", workout.id, log.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let workout_repo = WorkoutRepository::new(pool);
    let logs = workout_repo
        .find_logs_by_session_with_pr(&workout.id, &user.id, recent_pr_window_start())
        .await
        .unwrap();
    assert_eq!(logs.len(), 0);
}

#[tokio::test]
async fn test_delete_log_requires_ownership() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

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

    let session_cookie = common::create_session_cookie(&pool, &user1).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/logs/{}/delete", workout.id, log.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let workout_repo = WorkoutRepository::new(pool);
    let found = workout_repo.find_log_by_id(&log.id).await.unwrap();
    assert!(found.is_some());
}

// Log editing tests

#[tokio::test]
async fn test_edit_log_page_renders() {
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
    let log =
        common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, 100.0, Some(8)).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/workouts/{}/logs/{}/edit", workout.id, log.id))
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
#[allow(clippy::float_cmp, reason = "exact-value test assertion")]
async fn test_update_log_success() {
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
    let log = common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, 100.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/logs/{}", workout.id, log.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("reps=12&weight=110&rpe=9"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let workout_repo = WorkoutRepository::new(pool);
    let updated = workout_repo.find_log_by_id(&log.id).await.unwrap().unwrap();
    assert_eq!(updated.reps, 12);
    assert_eq!(updated.weight, 110.0);
    assert_eq!(updated.rpe, Some(9));
}

#[tokio::test]
#[allow(clippy::float_cmp, reason = "exact-value test assertion")]
async fn test_update_log_accepts_fractional_weight() {
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
    let log = common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, 100.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/logs/{}", workout.id, log.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("reps=10&weight=21.25&rpe=8"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let workout_repo = WorkoutRepository::new(pool);
    let updated = workout_repo.find_log_by_id(&log.id).await.unwrap().unwrap();
    assert_eq!(updated.weight, 21.25);
}

#[tokio::test]
#[allow(clippy::float_cmp, reason = "exact-value test assertion")]
async fn test_update_log_requires_ownership() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

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

    let session_cookie = common::create_session_cookie(&pool, &user1).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/logs/{}", workout.id, log.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("reps=12&weight=110&rpe=9"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let workout_repo = WorkoutRepository::new(pool);
    let found = workout_repo.find_log_by_id(&log.id).await.unwrap().unwrap();
    assert_eq!(found.reps, 10);
    assert_eq!(found.weight, 100.0);
}

// Pagination tests

#[tokio::test]
async fn test_workouts_list_pagination_page_2() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    // Create 15 workouts (more than one page of 10)
    #[allow(
        clippy::cast_sign_loss,
        reason = "loop counter 1..=15 is always positive"
    )]
    for i in 1..=15 {
        common::create_test_workout(
            &pool,
            &user.id,
            chrono::NaiveDate::from_ymd_opt(2024, 1, i as u32).unwrap(),
            Some(&format!("Workout {i}")),
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

/// Deleting a workout cascades to its sets, so the confirmation page has to
/// say so — and, being a GET, must not delete anything itself.
#[tokio::test]
async fn test_delete_workout_confirmation_page_names_the_cascade_without_acting() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);

    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let exercise = common::create_test_exercise(&pool, &user.id, "Squat", "Legs").await;
    common::create_test_log(&pool, &workout.id, &exercise.id, 1, 5, 100.0, None).await;
    common::create_test_log(&pool, &workout.id, &exercise.id, 2, 5, 105.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/workouts/{}/delete", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    assert!(
        body_str.contains("All 2 of its recorded sets will be deleted with it."),
        "the page must spell out the cascade, got: {body_str}"
    );
    assert!(
        body_str.contains(&format!(
            r#"<form method="post" action="/workouts/{}/delete">"#,
            workout.id
        )),
        "the page should post back to the same route"
    );

    // The GET must have been inert.
    let workout_repo = WorkoutRepository::new(pool.clone());
    assert_eq!(
        workout_repo.count_sessions_by_user(&user.id).await.unwrap(),
        1,
        "viewing the confirmation page must not delete the workout"
    );
}

/// The delete trigger has to carry both halves of the arrangement: an `href`
/// to the confirmation page (the path a browser with scripts off takes) and
/// a `data-confirm` for base.html to intercept (the path everyone else
/// takes). Losing the attribute would silently cost every JS user a page
/// load; losing the href would silently cost no-JS users the confirmation.
#[tokio::test]
async fn test_workout_delete_trigger_carries_both_confirmation_paths() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);

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
                .uri(format!("/workouts/{}", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    assert!(
        body_str.contains(&format!(r#"href="/workouts/{}/delete""#, workout.id)),
        "the trigger must link to the confirmation page"
    );
    assert!(
        body_str.contains(r#"data-confirm="Delete this workout?"#),
        "the trigger must carry the dialog text for the scripted path"
    );
    assert!(
        !body_str.contains("onsubmit"),
        "nothing on the page may fall back to an inline onsubmit guard"
    );
}

/// The set count is the reason this page exists, so all three phrasings —
/// none, one, many — are worth pinning. Grammar in generated prose is easy to
/// get wrong and nothing else would catch "Its 1 recorded sets".
#[tokio::test]
async fn test_delete_workout_confirmation_page_phrases_the_set_count() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);
    let exercise = common::create_test_exercise(&pool, &user.id, "Squat", "Legs").await;

    for (day, sets, expected) in [
        (1, 0, "It has no sets recorded."),
        (2, 1, "Its 1 recorded set will be deleted with it."),
        (3, 3, "All 3 of its recorded sets will be deleted with it."),
    ] {
        let workout = common::create_test_workout(
            &pool,
            &user.id,
            chrono::NaiveDate::from_ymd_opt(2024, 2, day).unwrap(),
            None,
        )
        .await;
        for set_number in 1..=sets {
            common::create_test_log(&pool, &workout.id, &exercise.id, set_number, 5, 100.0, None)
                .await;
        }

        let response = test_app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/workouts/{}/delete", workout.id))
                    .header(header::COOKIE, &cookie_header)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8_lossy(&body);
        assert!(
            body_str.contains(expected),
            "with {sets} set(s) the page should say {expected:?}, got: {body_str}"
        );
    }
}

/// A confirmation page for someone else's workout would leak that it exists,
/// and offer to delete it. It has to 404 exactly like the POST does.
#[tokio::test]
async fn test_delete_workout_confirmation_page_rejects_another_users_workout() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let owner = common::create_test_user(&pool, "owner", "password123", UserRole::User).await;
    let intruder = common::create_test_user(&pool, "intruder", "password123", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &intruder).await);

    let workout = common::create_test_workout(
        &pool,
        &owner.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/workouts/{}/delete", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// The set-delete page names the set it is about to remove, which means
/// resolving the log through the session that owns it.
#[tokio::test]
async fn test_delete_set_confirmation_page_names_the_set_without_acting() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);

    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "Chest").await;
    let log = common::create_test_log(&pool, &workout.id, &exercise.id, 3, 8, 80.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/workouts/{}/logs/{}/delete", workout.id, log.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    assert!(
        body_str.contains("Set 3 of Bench Press"),
        "the page must name the set, got: {body_str}"
    );

    // The GET must have been inert.
    let workout_repo = WorkoutRepository::new(pool.clone());
    let logs = workout_repo
        .find_logs_by_session_with_pr(&workout.id, &user.id, recent_pr_window_start())
        .await
        .unwrap();
    assert_eq!(
        logs.len(),
        1,
        "viewing the confirmation page must not delete the set"
    );
}

/// A log id that belongs to a different session must not render a page
/// offering to delete it.
#[tokio::test]
async fn test_delete_set_confirmation_page_rejects_a_log_from_another_session() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);

    let exercise = common::create_test_exercise(&pool, &user.id, "Squat", "Legs").await;
    let first = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let second = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 16).unwrap(),
        None,
    )
    .await;
    let log = common::create_test_log(&pool, &second.id, &exercise.id, 1, 5, 100.0, None).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/workouts/{}/logs/{}/delete", first.id, log.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
