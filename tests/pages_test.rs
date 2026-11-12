//! Page-level checks for views that other test files only reach on their main path.

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use liftlog::models::UserRole;
use liftlog::repositories::NewWorkoutLog;
use tower::ServiceExt;

fn date() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
}

/// GET `uri` as the given user; returns the status and body text.
async fn get_as(
    pool: &liftlog::db::DbPool,
    user: &liftlog::models::User,
    uri: &str,
) -> (StatusCode, String) {
    let test_app = common::create_test_app_with_session(pool.clone());
    let cookie = common::extract_cookie_header(&common::create_session_cookie(pool, user).await);
    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn new_exercise_page_renders_the_form() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "lifter", "password123", UserRole::User).await;

    let (status, body) = get_as(&pool, &user, "/exercises/new").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"action="/exercises""#));
}

#[tokio::test]
async fn new_workout_page_falls_back_to_the_browser_date_without_a_timezone() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "lifter", "password123", UserRole::User).await;

    let (status, body) = get_as(&pool, &user, "/workouts/new").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("data-server-today"));
    assert!(body.contains("formatLocalDate"));
}

#[tokio::test]
async fn workouts_list_paginates_with_previous_and_next_links() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "lifter", "password123", UserRole::User).await;
    for day in 1..=25 {
        let date = chrono::NaiveDate::from_ymd_opt(2024, 2, day).unwrap();
        common::create_test_workout(&pool, &user.id, date, None).await;
    }

    let (_, first) = get_as(&pool, &user, "/workouts").await;
    let (_, second) = get_as(&pool, &user, "/workouts?page=2").await;

    assert!(first.contains("/workouts?page=2"));
    assert!(!first.contains("Previous"));
    assert!(second.contains("/workouts?page=1"));
    assert!(second.contains("/workouts?page=3"));
}

#[tokio::test]
async fn dashboard_lists_recent_workouts_with_their_notes() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "lifter", "password123", UserRole::User).await;
    common::create_test_workout(&pool, &user.id, date(), Some("Heavy singles")).await;

    let (status, body) = get_as(&pool, &user, "/").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Heavy singles"));
}

#[tokio::test]
async fn users_page_offers_promote_only_for_non_admins() {
    let pool = common::setup_test_db();
    let admin = common::create_test_user(&pool, "admin", "adminpass", UserRole::Admin).await;
    let member = common::create_test_user(&pool, "member", "password123", UserRole::User).await;

    let (status, body) = get_as(&pool, &admin, "/users").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(&format!("/users/{}/promote", member.id)));
    assert!(!body.contains(&format!("/users/{}/promote", admin.id)));
}

#[tokio::test]
async fn prs_page_shows_the_empty_state() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "lifter", "password123", UserRole::User).await;

    let (status, body) = get_as(&pool, &user, "/stats/prs").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("No personal records yet"));
}

#[tokio::test]
async fn workout_page_shows_when_a_share_link_expires() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "lifter", "password123", UserRole::User).await;
    let workout = common::create_test_workout(&pool, &user.id, date(), None).await;
    let test_app = common::create_test_app_with_session(pool.clone());
    let cookie = common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);
    let share = |body: &'static str| {
        Request::builder()
            .method("POST")
            .uri(format!("/workouts/{}/share", workout.id))
            .header(header::COOKIE, &cookie)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body))
            .unwrap()
    };
    // An empty form has no `expires_in_days` key at all.
    let response = test_app.router.oneshot(share("")).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let (_, never) = get_as(&pool, &user, &format!("/workouts/{}", workout.id)).await;
    assert!(never.contains("Never expires"));

    let workout_repo = liftlog::repositories::WorkoutRepository::new(pool.clone());
    workout_repo
        .revoke_share_token(&workout.id, &user.id)
        .await
        .unwrap();
    let test_app = common::create_test_app_with_session(pool.clone());
    let response = test_app
        .router
        .oneshot(share("expires_in_days=7"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let (_, expiring) = get_as(&pool, &user, &format!("/workouts/{}", workout.id)).await;
    assert!(expiring.contains("Expires"));
    assert!(expiring.contains(r#"data-fmt="datetime""#));
}

#[tokio::test]
async fn edit_set_page_404s_for_a_set_of_another_workout() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "lifter", "password123", UserRole::User).await;
    let first = common::create_test_workout(&pool, &user.id, date(), None).await;
    let second = common::create_test_workout(&pool, &user.id, date(), None).await;
    let exercise = common::create_test_exercise(&pool, &user.id, "Squat", "legs").await;
    let log = common::create_test_log(
        &pool,
        NewWorkoutLog {
            session_id: &first.id,
            exercise_id: &exercise.id,
            set_number: 1,
            reps: 5,
            weight: 100.0,
            rpe: None,
        },
    )
    .await;

    let (status, _) = get_as(
        &pool,
        &user,
        &format!("/workouts/{}/logs/{}/edit", second.id, log.id),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
