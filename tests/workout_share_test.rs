mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
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
                .uri(format!("/workouts/{}/share", workout.id))
                .header(header::COOKIE, &cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::empty())
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

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        Some("Shared workout test"),
    )
    .await;
    common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, 100.0, Some(8)).await;

    let workout_repo = WorkoutRepository::new(pool.clone());
    let share_token = workout_repo
        .set_share_token(&workout.id, &user.id, None)
        .await
        .unwrap();

    // View shared workout without auth (new app instance to avoid cookies)
    let app = common::create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/shared/{share_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

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

    let workout_repo = WorkoutRepository::new(pool.clone());
    let share_token = workout_repo
        .set_share_token(&workout.id, &user.id, None)
        .await
        .unwrap();

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/revoke-share", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
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
    assert!(updated.share_token.is_none());

    let app = common::create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/shared/{share_token}"))
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

    let token1 = workout_repo
        .set_share_token(&workout.id, &user.id, None)
        .await
        .unwrap();

    workout_repo
        .revoke_share_token(&workout.id, &user.id)
        .await
        .unwrap();

    let token2 = workout_repo
        .set_share_token(&workout.id, &user.id, None)
        .await
        .unwrap();

    assert_ne!(token1, token2);

    let app = common::create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/shared/{token1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let app2 = common::create_test_app(pool.clone());
    let response2 = app2
        .oneshot(
            Request::builder()
                .uri(format!("/shared/{token2}"))
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
                .uri(format!("/workouts/{}/share", workout.id))
                .header(header::COOKIE, &cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

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
                .uri(format!("/workouts/{}/share", workout.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

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
                .uri(format!("/workouts/{}/revoke-share", workout.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_cannot_revoke_others_share() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    let workout = common::create_test_workout(
        &pool,
        &user2.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let workout_repo = WorkoutRepository::new(pool.clone());
    let share_token = workout_repo
        .set_share_token(&workout.id, &user2.id, None)
        .await
        .unwrap();

    let session_cookie = common::create_session_cookie(&pool, &user1).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/revoke-share", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

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

    assert!(body_str.contains(">Share</button>"));
    assert!(!body_str.contains("Revoke Share"));
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

    let workout_repo = WorkoutRepository::new(pool.clone());
    let share_token = workout_repo
        .set_share_token(&workout.id, &user.id, None)
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

    // A link to the confirmation page, not a POST button: the old
    // confirm() guard did nothing with JavaScript off.
    assert!(body_str.contains("Revoke Share</a>"));
    assert!(body_str.contains("Share link:"));
    assert!(body_str.contains(&format!("/shared/{share_token}")));
    // Should not show share button (only the revoke form, not the share form)
    assert!(!body_str.contains(">Share</button>"));
}

// Share expiry tests (migration 012)

#[tokio::test]
async fn test_share_without_expiry_never_expires() {
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

    // expires_in_days absent entirely — the "never expires" default.
    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/share", workout.id))
                .header(header::COOKIE, &cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let workout_repo = WorkoutRepository::new(pool.clone());
    let updated = workout_repo
        .find_session_by_id(&workout.id)
        .await
        .unwrap()
        .unwrap();
    assert!(updated.share_token.is_some());
    assert!(updated.share_expires_at.is_none());

    // Backward-compatibility guard: a NULL expiry must still resolve.
    let share_token = updated.share_token.unwrap();
    let app = common::create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/shared/{share_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_share_with_expiry_sets_share_expires_at() {
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
                .method("POST")
                .uri(format!("/workouts/{}/share", workout.id))
                .header(header::COOKIE, &cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("expires_in_days=7"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let workout_repo = WorkoutRepository::new(pool.clone());
    let updated = workout_repo
        .find_session_by_id(&workout.id)
        .await
        .unwrap()
        .unwrap();
    let expires_at = updated
        .share_expires_at
        .expect("share_expires_at should be set");
    let expected = chrono::Utc::now() + chrono::Duration::days(7);
    // A few seconds of drift, same tolerance neighbouring time-based tests allow.
    assert!((expires_at - expected).num_seconds().abs() < 5);

    // A future (not-yet-elapsed) expiry must still resolve. Every other test
    // in this file covers NULL expiry or a past expiry; nothing previously
    // proved a live, unexpired share link actually works — a regression
    // narrowing find_session_by_share_token's predicate to just
    // `share_expires_at IS NULL` would make every expiring link dead on
    // creation while leaving the rest of the suite green.
    let share_token = updated.share_token.unwrap();
    let app = common::create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/shared/{share_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("2024-01-15") || body_str.contains("testuser"));
}

#[tokio::test]
async fn test_expired_share_token_returns_404() {
    let pool = common::setup_test_db();

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        Some("Secret expired workout"),
    )
    .await;

    let workout_repo = WorkoutRepository::new(pool.clone());
    let share_token = workout_repo
        .set_share_token(&workout.id, &user.id, Some(chrono::Duration::days(7)))
        .await
        .unwrap();

    // Age the row past expiry, following `expire_session`'s technique in
    // tests/common/mod.rs.
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE workout_sessions SET share_expires_at = datetime('now', '-1 hour') WHERE id = ?",
            [&workout.id],
        )
        .unwrap();
    }

    let app = common::create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/shared/{share_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    // An expired token and a never-issued one must be indistinguishable —
    // nothing about the workout itself should leak into the response.
    assert!(!body_str.contains("Secret expired workout"));
    assert!(!body_str.contains("Shared by"));
}

#[tokio::test]
async fn test_revoke_share_clears_expiry_too() {
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

    let workout_repo = WorkoutRepository::new(pool.clone());
    workout_repo
        .set_share_token(&workout.id, &user.id, Some(chrono::Duration::days(7)))
        .await
        .unwrap();

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workouts/{}/revoke-share", workout.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
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
    assert!(updated.share_token.is_none());
    assert!(updated.share_expires_at.is_none());
}

#[tokio::test]
async fn test_share_rejects_out_of_range_expiry() {
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

    for invalid in ["0", "400"] {
        let response = test_app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/workouts/{}/share", workout.id))
                    .header(header::COOKIE, &cookie_header)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!("expires_in_days={invalid}")))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "expires_in_days={invalid}"
        );
    }

    // Neither attempt should have shared the workout.
    let workout_repo = WorkoutRepository::new(pool);
    let found = workout_repo
        .find_session_by_id(&workout.id)
        .await
        .unwrap()
        .unwrap();
    assert!(found.share_token.is_none());
}

#[tokio::test]
async fn test_cleanup_expired_share_tokens_nulls_them() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let workout_repo = WorkoutRepository::new(pool.clone());

    let expired = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;
    let never_expires = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 16).unwrap(),
        None,
    )
    .await;
    let future_expiry = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 17).unwrap(),
        None,
    )
    .await;

    workout_repo
        .set_share_token(&expired.id, &user.id, Some(chrono::Duration::days(7)))
        .await
        .unwrap();
    workout_repo
        .set_share_token(&never_expires.id, &user.id, None)
        .await
        .unwrap();
    workout_repo
        .set_share_token(
            &future_expiry.id,
            &user.id,
            Some(chrono::Duration::days(30)),
        )
        .await
        .unwrap();

    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE workout_sessions SET share_expires_at = datetime('now', '-1 hour') WHERE id = ?",
            [&expired.id],
        )
        .unwrap();
    }

    let cleared = workout_repo.cleanup_expired_share_tokens().await.unwrap();
    assert_eq!(cleared, 1);

    let expired_row = workout_repo
        .find_session_by_id(&expired.id)
        .await
        .unwrap()
        .unwrap();
    assert!(expired_row.share_token.is_none());
    assert!(expired_row.share_expires_at.is_none());

    let never_row = workout_repo
        .find_session_by_id(&never_expires.id)
        .await
        .unwrap()
        .unwrap();
    assert!(never_row.share_token.is_some());
    assert!(never_row.share_expires_at.is_none());

    let future_row = workout_repo
        .find_session_by_id(&future_expiry.id)
        .await
        .unwrap()
        .unwrap();
    assert!(future_row.share_token.is_some());
    assert!(future_row.share_expires_at.is_some());
}

/// Revoking is irreversible for anyone holding the old link, which the old
/// `confirm()` said and — with JavaScript off — never asked. The interstitial
/// must say it, and must leave the token alone until the POST.
#[tokio::test]
async fn test_revoke_share_confirmation_page_warns_without_revoking() {
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

    let workout_repo = WorkoutRepository::new(pool.clone());
    let share_token = workout_repo
        .set_share_token(&workout.id, &user.id, None)
        .await
        .unwrap();

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/workouts/{}/revoke-share", workout.id))
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
        body_str.contains("will stop working for anyone who already has it"),
        "the page must warn that the existing link dies, got: {body_str}"
    );

    // The GET must have been inert: the share link still resolves.
    let row = workout_repo
        .find_session_by_share_token(&share_token)
        .await
        .unwrap();
    assert!(
        row.is_some(),
        "viewing the confirmation page must not revoke the token"
    );
}
