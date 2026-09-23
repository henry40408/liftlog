mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use liftlog::models::UserRole;
use tower::ServiceExt;

#[tokio::test]
async fn test_login_page_redirects_to_setup_when_no_users() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/setup");
}

/// Deleting a user must also delete their sessions (via the FK cascade or the
/// handler's explicit cleanup).
#[tokio::test]
async fn test_admin_delete_user_removes_their_sessions() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass123", UserRole::Admin).await;
    let admin_cookie = common::create_session_cookie(&pool, &admin).await;
    let admin_cookie_header = common::extract_cookie_header(&admin_cookie);

    let victim = common::create_test_user(&pool, "victim", "victimpass123", UserRole::User).await;
    let _victim_token1 = common::create_session_token(&pool, &victim).await;
    let _victim_token2 = common::create_session_token(&pool, &victim).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/delete", victim.id))
                .header(header::COOKIE, &admin_cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("current_password=adminpass123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/users");

    let conn = pool.get().unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE user_id = ?",
            [&victim.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0, "deleted user's sessions should all be gone");
}

/// Regression: sessions used to be deleted before the user row, so a failed
/// delete left the user alive with no sessions.
#[tokio::test]
async fn test_admin_delete_user_keeps_sessions_when_user_delete_fails() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass123", UserRole::Admin).await;
    let admin_cookie = common::create_session_cookie(&pool, &admin).await;
    let admin_cookie_header = common::extract_cookie_header(&admin_cookie);

    let victim = common::create_test_user(&pool, "victim", "victimpass123", UserRole::User).await;
    let _victim_token1 = common::create_session_token(&pool, &victim).await;
    let _victim_token2 = common::create_session_token(&pool, &victim).await;

    {
        let conn = pool.get().unwrap();
        conn.execute(
            &format!(
                "CREATE TRIGGER block_victim_delete BEFORE DELETE ON users \
                 WHEN OLD.id = '{}' \
                 BEGIN SELECT RAISE(ABORT, 'blocked'); END",
                victim.id
            ),
            [],
        )
        .unwrap();
    }

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/delete", victim.id))
                .header(header::COOKIE, &admin_cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("current_password=adminpass123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    {
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE user_id = ?",
                [&victim.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 2,
            "victim's sessions should still exist since user delete failed"
        );
    }

    {
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM users WHERE id = ?",
                [&victim.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "victim's user row should still exist since delete failed"
        );
    }
}

/// Exercises the handler's explicit session cleanup with the cascade disabled.
/// The test pool is a single connection (`max_size(1)`), so this pragma also
/// reaches the handler's connection.
#[tokio::test]
async fn test_admin_delete_user_removes_sessions_without_foreign_keys() {
    let pool = common::setup_test_db();
    {
        let conn = pool.get().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    }
    let test_app = common::create_test_app_with_session(pool.clone());

    let admin = common::create_test_user(&pool, "admin", "adminpass123", UserRole::Admin).await;
    let admin_cookie = common::create_session_cookie(&pool, &admin).await;
    let admin_cookie_header = common::extract_cookie_header(&admin_cookie);

    let victim = common::create_test_user(&pool, "victim", "victimpass123", UserRole::User).await;
    let _victim_token1 = common::create_session_token(&pool, &victim).await;
    let _victim_token2 = common::create_session_token(&pool, &victim).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/delete", victim.id))
                .header(header::COOKIE, &admin_cookie_header)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("current_password=adminpass123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/users");

    let conn = pool.get().unwrap();
    let session_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE user_id = ?",
            [&victim.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        session_count, 0,
        "the explicit cleanup should remove the sessions without the cascade"
    );

    let user_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM users WHERE id = ?",
            [&victim.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(user_count, 0, "victim's user row should be gone");
}

/// Isolates the cascade: a bare `DELETE FROM users`, no handler involved.
#[tokio::test]
async fn test_deleting_user_cascades_their_sessions() {
    let pool = common::setup_test_db();

    let victim = common::create_test_user(&pool, "victim", "victimpass123", UserRole::User).await;
    let _victim_token1 = common::create_session_token(&pool, &victim).await;
    let _victim_token2 = common::create_session_token(&pool, &victim).await;

    {
        let conn = pool.get().unwrap();
        conn.execute("DELETE FROM users WHERE id = ?", [&victim.id])
            .unwrap();
    }

    let conn = pool.get().unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE user_id = ?",
            [&victim.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0, "ON DELETE CASCADE should remove the sessions");
}

#[tokio::test]
async fn test_setup_page_available_when_no_users() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/setup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_dashboard_requires_auth() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_login_valid_credentials() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=password123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/");

    let set_cookie = response.headers().get(header::SET_COOKIE);
    assert!(set_cookie.is_some());
    let cookie_str = set_cookie.unwrap().to_str().unwrap();
    assert!(cookie_str.contains("session="));
}

#[tokio::test]
async fn test_login_invalid_credentials() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=wrongpassword"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("Invalid username or password"));
}

#[tokio::test]
async fn test_login_nonexistent_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    common::create_test_user(&pool, "existing", "password", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=nonexistent&password=anypassword"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("Invalid username or password"));
}

#[tokio::test]
async fn test_logout_clears_session() {
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
                .uri("/auth/logout")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");

    let set_cookie = response.headers().get(header::SET_COOKIE);
    assert!(set_cookie.is_some());
    let cookie_str = set_cookie.unwrap().to_str().unwrap();
    assert!(cookie_str.contains("Max-Age=0") || cookie_str.contains("session=;"));
}

#[tokio::test]
async fn test_logout_sets_clear_site_data() {
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
                .uri("/auth/logout")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Exact match: browsers ignore the whole header if a directive is unquoted.
    let clear_site_data = response
        .headers()
        .get("clear-site-data")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(clear_site_data, "\"cache\", \"cookies\", \"storage\"");
}

#[tokio::test]
async fn test_logout_still_sends_removal_cookie() {
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
                .uri("/auth/logout")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let set_cookie = response.headers().get(header::SET_COOKIE);
    assert!(set_cookie.is_some());
    let cookie_str = set_cookie.unwrap().to_str().unwrap();
    assert!(cookie_str.contains("Max-Age=0") || cookie_str.contains("session=;"));
}

#[tokio::test]
async fn test_setup_creates_admin_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/setup")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "username=admin&password={}",
                    common::STRONG_PASSWORD
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/");

    let user_repo = liftlog::repositories::UserRepository::new(pool);
    let user = user_repo.find_by_username("admin").await.unwrap();
    assert!(user.is_some());
    assert_eq!(user.unwrap().role, UserRole::Admin);
}

#[tokio::test]
async fn test_setup_rejects_empty_username() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/setup")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "username=&password={}",
                    common::STRONG_PASSWORD
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("Username is required"));

    let user_repo = liftlog::repositories::UserRepository::new(pool);
    let count = user_repo.count().await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_setup_rejects_short_password() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/setup")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=admin&password=short"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("Password must be at least 12 characters"));

    let user_repo = liftlog::repositories::UserRepository::new(pool);
    let count = user_repo.count().await.unwrap();
    assert_eq!(count, 0);
}

/// Pins the exact boundary so an off-by-one cannot pass.
#[tokio::test]
async fn test_setup_password_length_boundary() {
    for (password, should_create) in [("gymrat.2026", false), ("gymrat.2026!", true)] {
        let pool = common::setup_test_db();
        let test_app = common::create_test_app_with_session(pool.clone());

        let response = test_app
            .router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/setup")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!("username=admin&password={password}")))
                    .unwrap(),
            )
            .await
            .unwrap();

        let user_repo = liftlog::repositories::UserRepository::new(pool);
        let count = user_repo.count().await.unwrap();
        assert_eq!(
            count,
            i64::from(should_create),
            "password {:?} ({} chars) should {}have created a user",
            password,
            password.chars().count(),
            if should_create { "" } else { "not " }
        );
        assert_eq!(
            response.status(),
            if should_create {
                StatusCode::SEE_OTHER
            } else {
                StatusCode::OK
            }
        );
    }
}

#[tokio::test]
async fn test_setup_redirects_when_users_exist() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    common::create_test_user(&pool, "existing", "password", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/auth/setup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_sliding_session_no_cookie_when_within_throttle() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    // Fresh session: within the touch throttle.
    let token = common::create_session_token(&pool, &user).await;

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, common::cookie_header(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers().get(header::SET_COOKIE).is_none(),
        "cookie should NOT be re-issued within throttle window"
    );
}

#[tokio::test]
async fn test_expired_session_redirects_to_login() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let token = common::create_session_token(&pool, &user).await;
    common::expire_session(&pool, &token);

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, common::cookie_header(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_over_age_session_redirects_to_login() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let token = common::create_session_token(&pool, &user).await;
    // Past the 90-day absolute cap, though the idle expiry is still ahead.
    common::age_session_creation(&pool, &token, 91);

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, common::cookie_header(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_login_rate_limited_after_max_attempts() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_rate_limit(
        pool.clone(),
        3,
        std::time::Duration::from_secs(60),
    );

    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    for _ in 0..3 {
        let response = test_app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("username=testuser&password=wrongpassword"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = test_app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=wrongpassword"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("Too many login attempts"));
}

#[tokio::test]
async fn test_successful_login_releases_its_attempt() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_rate_limit(
        pool.clone(),
        2,
        std::time::Duration::from_secs(60),
    );

    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    for _ in 0..5 {
        let response = test_app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("username=testuser&password=password123"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
    }
}

#[tokio::test]
async fn test_login_succeeds_when_under_limit() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=password123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/");
}

#[tokio::test]
async fn test_login_set_cookie_has_secure_when_cookie_secure_enabled() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_cookie_secure(pool.clone(), true);

    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=password123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(set_cookie.contains("Secure"), "got: {set_cookie}");
}

#[tokio::test]
async fn test_login_set_cookie_omits_secure_by_default() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=password123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(!set_cookie.contains("Secure"), "got: {set_cookie}");
}

#[tokio::test]
async fn test_logout_removal_cookie_has_secure_when_enabled() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_cookie_secure(pool.clone(), true);

    // The secure app only accepts the __Host- prefixed cookie name.
    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let token = common::create_session_token(&pool, &user).await;
    let cookie_header = common::cookie_header_secure(&token);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(set_cookie.contains("Secure"), "got: {set_cookie}");
}

#[tokio::test]
async fn test_login_uses_host_prefixed_cookie_when_secure() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_cookie_secure(pool.clone(), true);

    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=password123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        set_cookie.starts_with("__Host-session="),
        "got: {set_cookie}"
    );
}

#[tokio::test]
async fn test_host_prefixed_cookie_is_accepted_on_subsequent_request() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;
    let token = common::create_session_token(&pool, &user).await;

    let test_app = common::create_test_app_with_cookie_secure(pool, true);
    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, common::cookie_header_secure(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_plain_session_cookie_rejected_by_secure_app() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;
    let token = common::create_session_token(&pool, &user).await;

    let test_app = common::create_test_app_with_cookie_secure(pool, true);
    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, common::cookie_header(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/auth/login");
}

#[tokio::test]
async fn test_login_page_redirects_to_dashboard_when_already_authenticated() {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let token = common::create_session_token(&pool, &user).await;

    let app = common::create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .header(header::COOKIE, common::cookie_header(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/");
}

// `oneshot` attaches no `ConnectInfo`, so the per-IP tests below add one via
// `common::with_peer`.

/// A failing login POST (so the limiter is never released).
fn wrong_password_login_request(headers: &[(&str, &str)]) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    builder
        .body(Body::from("username=testuser&password=wrongpassword"))
        .unwrap()
}

#[tokio::test]
async fn test_distinct_peers_have_independent_login_budgets() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_rate_limit(
        pool.clone(),
        1,
        std::time::Duration::from_secs(60),
    );
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[]),
            "203.0.113.1:1111",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[]),
            "203.0.113.1:1111",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[]),
            "203.0.113.2:2222",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_untrusted_peer_forged_xff_is_ignored() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_proxy_header(
        pool.clone(),
        1,
        std::time::Duration::from_secs(60),
        liftlog::config::TrustedProxyHeader::XForwardedFor,
        Vec::new(),
    );
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[("x-forwarded-for", "1.1.1.1")]),
            "203.0.113.9:1234",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Same untrusted peer, different forged XFF: same bucket.
    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[("x-forwarded-for", "2.2.2.2")]),
            "203.0.113.9:1234",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_loopback_peer_honours_rightmost_xff_hop() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_proxy_header(
        pool.clone(),
        1,
        std::time::Duration::from_secs(60),
        liftlog::config::TrustedProxyHeader::XForwardedFor,
        Vec::new(),
    );
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[("x-forwarded-for", "9.9.9.9, 10.1.1.1")]),
            "127.0.0.1:1111",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Different rightmost hop: different bucket.
    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[("x-forwarded-for", "9.9.9.9, 10.1.1.2")]),
            "127.0.0.1:1111",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Same rightmost hop, different leftmost: same bucket.
    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[("x-forwarded-for", "8.8.8.8, 10.1.1.1")]),
            "127.0.0.1:1111",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

/// Repeated `X-Forwarded-For` lines bucket by the last line. Reading the first
/// line, or ignoring the header, would make the second request 429.
#[tokio::test]
async fn test_duplicate_xff_header_lines_bucket_by_the_last_line() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_proxy_header(
        pool.clone(),
        1,
        std::time::Duration::from_secs(60),
        liftlog::config::TrustedProxyHeader::XForwardedFor,
        Vec::new(),
    );
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[
                ("x-forwarded-for", "1.1.1.1"),
                ("x-forwarded-for", "198.51.100.7"),
            ]),
            "127.0.0.1:1111",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[
                ("x-forwarded-for", "1.1.1.1"),
                ("x-forwarded-for", "198.51.100.8"),
            ]),
            "127.0.0.1:1111",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[
                ("x-forwarded-for", "9.9.9.9"),
                ("x-forwarded-for", "198.51.100.7"),
            ]),
            "127.0.0.1:1111",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

/// With `LIFTLOG_TRUSTED_PROXY_HEADER` unset, a forged `X-Forwarded-For` must
/// not mint a fresh bucket, even from loopback.
#[tokio::test]
async fn test_forwarding_header_ignored_when_not_configured() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_rate_limit(
        pool.clone(),
        1,
        std::time::Duration::from_secs(60),
    );
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[("x-forwarded-for", "1.1.1.1")]),
            "127.0.0.1:1111",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Different forged XFF, same peer: same bucket.
    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[("x-forwarded-for", "2.2.2.2")]),
            "127.0.0.1:1111",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

/// With `X-Forwarded-For` selected, `X-Real-IP` is ignored even when XFF is absent.
#[tokio::test]
async fn test_x_real_ip_not_honoured_when_header_is_x_forwarded_for() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_proxy_header(
        pool.clone(),
        1,
        std::time::Duration::from_secs(60),
        liftlog::config::TrustedProxyHeader::XForwardedFor,
        Vec::new(),
    );
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[("x-real-ip", "1.1.1.1")]),
            "127.0.0.1:1111",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Different X-Real-IP, no XFF: still the peer's bucket.
    let response = test_app
        .router
        .clone()
        .oneshot(common::with_peer(
            wrong_password_login_request(&[("x-real-ip", "2.2.2.2")]),
            "127.0.0.1:1111",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

async fn sliding_session_reissues_cookie_when_throttle_elapsed(cookie_secure: bool) {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    let token = common::create_session_token(&pool, &user).await;
    common::age_session_touch(&pool, &token, 2);

    let test_app = common::create_test_app_with_cookie_secure(pool, cookie_secure);
    let cookie_header = if cookie_secure {
        common::cookie_header_secure(&token)
    } else {
        common::cookie_header(&token)
    };

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::SEE_OTHER);

    // Cookie re-issued with a fresh Max-Age.
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("sliding session should set cookie on touch")
        .to_str()
        .unwrap();
    let expected_name = liftlog::session::session_cookie_name(cookie_secure);
    assert!(
        set_cookie.starts_with(&format!("{expected_name}=")),
        "got: {set_cookie}"
    );
    assert!(set_cookie.contains("Max-Age=604800")); // 7 days in seconds
}

#[tokio::test]
async fn test_sliding_session_reissues_cookie_when_throttle_elapsed() {
    sliding_session_reissues_cookie_when_throttle_elapsed(false).await;
}

#[tokio::test]
async fn test_sliding_session_reissues_cookie_when_throttle_elapsed_secure() {
    sliding_session_reissues_cookie_when_throttle_elapsed(true).await;
}

async fn logout_does_not_get_overridden_by_sliding_refresh(cookie_secure: bool) {
    let pool = common::setup_test_db();
    let user = common::create_test_user(&pool, "alice", "password123", UserRole::User).await;

    // Age the session so the next request triggers a touch.
    let token = common::create_session_token(&pool, &user).await;
    common::age_session_touch(&pool, &token, 2);

    let test_app = common::create_test_app_with_cookie_secure(pool, cookie_secure);
    let cookie_header = if cookie_secure {
        common::cookie_header_secure(&token)
    } else {
        common::cookie_header(&token)
    };

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header(header::COOKIE, cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Exactly one session Set-Cookie, and it is the removal.
    let expected_name = liftlog::session::session_cookie_name(cookie_secure);
    let prefix = format!("{expected_name}=");
    let session_cookies: Vec<_> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter(|s| s.trim_start().starts_with(&prefix))
        .collect();
    assert_eq!(
        session_cookies.len(),
        1,
        "logout should emit exactly one session Set-Cookie header, got: {session_cookies:?}"
    );
    let only = session_cookies[0];
    assert!(
        only.contains("Max-Age=0"),
        "logout cookie should be the removal (Max-Age=0), got: {only}"
    );
}

#[tokio::test]
async fn test_logout_does_not_get_overridden_by_sliding_refresh() {
    logout_does_not_get_overridden_by_sliding_refresh(false).await;
}

#[tokio::test]
async fn test_logout_does_not_get_overridden_by_sliding_refresh_secure() {
    logout_does_not_get_overridden_by_sliding_refresh(true).await;
}

/// `LIFTLOG_COOKIE_SECURE=true` end to end: login, slide and logout all use
/// `__Host-session`. Losing `SessionLayerState.cookie_secure` would re-issue a
/// plain `session=` cookie that is never read, logging users out after 7 days.
#[tokio::test]
async fn test_secure_cookie_end_to_end_login_sliding_refresh_logout() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_cookie_secure(pool.clone(), true);
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=password123"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        set_cookie.starts_with("__Host-session="),
        "got: {set_cookie}"
    );
    let issued = common::extract_cookie_header(set_cookie);
    let token = issued
        .strip_prefix("__Host-session=")
        .expect("login should issue the __Host- prefixed cookie")
        .to_string();
    let cookie_header = common::cookie_header_secure(&token);

    // Age the session so the next request triggers a sliding touch.
    common::age_session_touch(&pool, &token, 2);

    let response = test_app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(response.status(), StatusCode::SEE_OTHER);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("sliding session should reissue the secure cookie on touch")
        .to_str()
        .unwrap();
    assert!(
        set_cookie.starts_with("__Host-session="),
        "got: {set_cookie}"
    );
    assert!(set_cookie.contains("Max-Age=604800"));

    let response = test_app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        set_cookie.starts_with("__Host-session="),
        "got: {set_cookie}"
    );
    assert!(set_cookie.contains("Max-Age=0"));
}

/// Authenticated pages must not be cached, so Back can't reveal them after logout.
#[tokio::test]
async fn test_authenticated_page_sets_no_store() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "no-cache, no-store, must-revalidate"
    );
    assert_eq!(response.headers().get("pragma").unwrap(), "no-cache");
}

/// No-store is scoped to authenticated requests; the login page stays cacheable.
#[tokio::test]
async fn test_unauthenticated_login_page_has_no_cache_control() {
    let pool = common::setup_test_db();
    // Otherwise /auth/login redirects to /auth/setup.
    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("cache-control").is_none());
}

/// The login response carries a new session cookie but its request had no
/// session, so the middleware's no-store never applies; `login_submit` sets it.
#[tokio::test]
async fn test_login_success_response_is_not_cacheable() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=password123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(response.headers().get(header::SET_COOKIE).is_some());
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "no-cache, no-store, must-revalidate"
    );
    assert_eq!(response.headers().get("pragma").unwrap(), "no-cache");
}

/// As `test_login_success_response_is_not_cacheable`, for `setup_submit`.
#[tokio::test]
async fn test_setup_success_response_is_not_cacheable() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/setup")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "username=admin&password={}",
                    common::STRONG_PASSWORD
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(response.headers().get(header::SET_COOKIE).is_some());
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "no-cache, no-store, must-revalidate"
    );
    assert_eq!(response.headers().get("pragma").unwrap(), "no-cache");
}

/// HSTS is opt-in: off by default.
#[tokio::test]
async fn test_no_hsts_header_by_default() {
    let pool = common::setup_test_db();
    let app = common::create_test_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response
            .headers()
            .get("strict-transport-security")
            .is_none()
    );
}

#[tokio::test]
async fn test_hsts_header_present_when_configured() {
    let pool = common::setup_test_db();
    let app = common::create_test_app_with_hsts(pool, 31_536_000, false).router;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.headers().get("strict-transport-security").unwrap(),
        "max-age=31536000"
    );
}

#[tokio::test]
async fn test_hsts_header_includes_subdomains_when_configured() {
    let pool = common::setup_test_db();
    let app = common::create_test_app_with_hsts(pool, 31_536_000, true).router;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.headers().get("strict-transport-security").unwrap(),
        "max-age=31536000; includeSubDomains"
    );
}

/// HSTS must be outside the CSRF guard so it also lands on its 403.
#[tokio::test]
async fn test_hsts_header_present_on_csrf_rejection() {
    let pool = common::setup_test_db();
    let app = common::create_test_app_with_hsts(pool, 31_536_000, false).router;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workouts")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header("sec-fetch-site", "cross-site")
                .body(Body::from("date=2024-01-15&notes=Leg%20day"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.headers().get("strict-transport-security").unwrap(),
        "max-age=31536000"
    );
}

/// The baseline headers are unconditional, unlike HSTS.
fn assert_baseline_headers(headers: &axum::http::HeaderMap) {
    assert_eq!(
        headers.get("content-security-policy").unwrap(),
        "frame-ancestors 'none'"
    );
    assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(
        headers.get("referrer-policy").unwrap(),
        "strict-origin-when-cross-origin"
    );
}

#[tokio::test]
async fn test_baseline_security_headers_on_a_normal_response() {
    let pool = common::setup_test_db();
    let app = common::create_test_app_with_session(pool).router;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_baseline_headers(response.headers());
}

/// Baseline headers must also cover responses that short-circuit in
/// middleware: the CSRF 403 and the auth 302.
#[tokio::test]
async fn test_baseline_security_headers_on_csrf_rejection() {
    let pool = common::setup_test_db();
    let app = common::create_test_app_with_session(pool).router;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workouts")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header("sec-fetch-site", "cross-site")
                .body(Body::from("date=2024-01-15&notes=Leg%20day"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_baseline_headers(response.headers());
}

#[tokio::test]
async fn test_baseline_security_headers_on_auth_redirect() {
    let pool = common::setup_test_db();
    let app = common::create_test_app_with_session(pool).router;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_baseline_headers(response.headers());
}

/// The public share page is the likeliest to be embedded; no exception.
#[tokio::test]
async fn test_baseline_security_headers_on_the_public_share_route() {
    let pool = common::setup_test_db();
    let app = common::create_test_app_with_session(pool).router;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/shared/no-such-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_baseline_headers(response.headers());
}

/// The maximum bounds hashing cost; over-long input is rejected, never truncated.
#[tokio::test]
async fn test_setup_rejects_over_long_password() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let too_long = "a".repeat(liftlog::models::user::MAX_PASSWORD_LEN + 1);
    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/setup")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("username=admin&password={too_long}")))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("at most 128 characters"),
        "expected the maximum-length message, got: {body_str}"
    );

    let user_repo = liftlog::repositories::UserRepository::new(pool.clone());
    assert_eq!(
        user_repo.count().await.unwrap(),
        0,
        "no user should have been created"
    );
}

/// `MyPassword12` passes length and composition rules, so only the strength
/// check (`password_policy_error`) can reject it.
#[tokio::test]
async fn test_setup_rejects_a_guessable_password() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/setup")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=admin&password=MyPassword12"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(
        !html.contains("must be at least"),
        "should be rejected on strength, not length: {html}"
    );

    let user_repo = liftlog::repositories::UserRepository::new(pool);
    assert_eq!(user_repo.count().await.unwrap(), 0);
}

/// `henrylifts.42x` is strong on its own, so rejection proves the handler
/// passes the username into the strength check.
#[tokio::test]
async fn test_setup_rejects_a_password_derived_from_the_username() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/setup")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=henrylifts&password=henrylifts.42x"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let user_repo = liftlog::repositories::UserRepository::new(pool);
    assert_eq!(
        user_repo.count().await.unwrap(),
        0,
        "a password built from the username must be rejected"
    );
}

/// Per-account backoff against a spray from many peers; the per-IP limit is
/// generous so only the account counter can cause the delay. Only a lower
/// bound is asserted, since an upper bound would flake on slow CI.
#[tokio::test]
async fn test_repeated_failures_against_one_account_are_delayed() {
    let pool = common::setup_test_db();
    // One free attempt, then 150ms, 300ms, …
    let base = std::time::Duration::from_millis(150);
    let test_app = common::create_test_app_with_login_backoff(pool.clone(), 1, base);
    common::create_test_user(&pool, "victim", "password123", UserRole::User).await;

    let attempt = |peer: &str| {
        common::with_peer(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("username=victim&password=wrongpass"))
                .unwrap(),
            peer,
        )
    };

    let response = test_app
        .router
        .clone()
        .oneshot(attempt("203.0.113.1:1111"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Second attempt, from another address, must wait.
    let started = std::time::Instant::now();
    let response = test_app
        .router
        .clone()
        .oneshot(attempt("203.0.113.2:2222"))
        .await
        .unwrap();
    let elapsed = started.elapsed();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        elapsed >= base,
        "expected the attempt to be held for at least {base:?}, took {elapsed:?}"
    );
}

/// Sign Out must be a real submit button so it works without JavaScript.
#[tokio::test]
async fn test_sign_out_button_works_without_javascript() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie = common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);

    assert!(
        html.contains(r#"<form action="/auth/logout" method="post""#),
        "the nav must post to /auth/logout with a plain form"
    );
    assert!(
        html.contains(r#"<button type="submit" class="sign-out-btn">"#),
        "Sign Out must submit the form itself, not rely on an onclick handler"
    );
}
