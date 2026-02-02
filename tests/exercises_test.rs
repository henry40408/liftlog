mod common;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use liftlog::models::UserRole;
use liftlog::repositories::ExerciseRepository;
use tower::ServiceExt;

// Auth tests

#[tokio::test]
async fn test_exercises_list_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/exercises")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_exercises_new_page_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/exercises/new")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_create_exercise_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exercises")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("name=Bench%20Press&category=chest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

// CRUD tests

#[tokio::test]
async fn test_create_exercise_success() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exercises")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("name=Bench%20Press&category=chest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/exercises");

    // Verify exercise was created
    let exercise_repo = ExerciseRepository::new(pool);
    let exercises = exercise_repo
        .find_available_for_user(&user.id)
        .await
        .unwrap();
    assert_eq!(exercises.len(), 1);
    assert_eq!(exercises[0].name, "Bench Press");
    assert_eq!(exercises[0].category, "chest");
}

#[tokio::test]
async fn test_exercises_list_shows_exercises() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    // Create some exercises
    common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
    common::create_test_exercise(&pool, &user.id, "Squat", "legs").await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/exercises")
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
}

#[tokio::test]
async fn test_edit_exercise_page_renders() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(&format!("/exercises/{}/edit", exercise.id))
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
}

#[tokio::test]
async fn test_update_exercise_success() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/exercises/{}", exercise.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("name=Incline%20Bench%20Press&category=chest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/exercises");

    // Verify exercise was updated
    let exercise_repo = ExerciseRepository::new(pool);
    let updated = exercise_repo
        .find_by_id(&exercise.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.name, "Incline Bench Press");
}

#[tokio::test]
async fn test_delete_exercise_success() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/exercises/{}/delete", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/exercises");

    // Verify exercise was deleted
    let exercise_repo = ExerciseRepository::new(pool);
    let found = exercise_repo.find_by_id(&exercise.id).await.unwrap();
    assert!(found.is_none());
}

// Authorization tests

#[tokio::test]
async fn test_cannot_edit_others_exercise() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    // Create exercise for user2
    let exercise = common::create_test_exercise(&pool, &user2.id, "Bench Press", "chest").await;

    // Login as user1
    let session_cookie = common::create_session_cookie(&user1, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(&format!("/exercises/{}/edit", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_cannot_update_others_exercise() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    let exercise = common::create_test_exercise(&pool, &user2.id, "Bench Press", "chest").await;

    let session_cookie = common::create_session_cookie(&user1, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/exercises/{}", exercise.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("name=Hacked&category=chest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Verify exercise was NOT updated
    let exercise_repo = ExerciseRepository::new(pool);
    let found = exercise_repo
        .find_by_id(&exercise.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.name, "Bench Press");
}

#[tokio::test]
async fn test_cannot_delete_others_exercise() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user1 = common::create_test_user(&pool, "user1", "password123", UserRole::User).await;
    let user2 = common::create_test_user(&pool, "user2", "password456", UserRole::User).await;

    let exercise = common::create_test_exercise(&pool, &user2.id, "Bench Press", "chest").await;

    let session_cookie = common::create_session_cookie(&user1, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/exercises/{}/delete", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Verify exercise was NOT deleted
    let exercise_repo = ExerciseRepository::new(pool);
    let found = exercise_repo.find_by_id(&exercise.id).await.unwrap();
    assert!(found.is_some());
}

// Not found tests

#[tokio::test]
async fn test_edit_nonexistent_exercise() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/exercises/nonexistent-id/edit")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_nonexistent_exercise() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exercises/nonexistent-id")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("name=Test&category=chest"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_nonexistent_exercise() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exercises/nonexistent-id/delete")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// Validation tests

#[tokio::test]
async fn test_create_exercise_empty_name_rejected() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exercises")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("name=&category=chest"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 200 with error message (re-renders form)
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    assert!(body_str.contains("required") || body_str.contains("Exercise name"));
}

#[tokio::test]
async fn test_update_exercise_empty_name_rejected() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_key(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&user, &test_app.session_key);
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let exercise = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/exercises/{}", exercise.id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie_header)
                .body(Body::from("name=&category=chest"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 200 with error message (re-renders form)
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    assert!(body_str.contains("required") || body_str.contains("Exercise name"));

    // Verify exercise was NOT updated
    let exercise_repo = ExerciseRepository::new(pool);
    let found = exercise_repo
        .find_by_id(&exercise.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.name, "Bench Press");
}
