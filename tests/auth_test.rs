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

/// An admin deleting a user must also delete every session belonging to
/// that user. `sessions.user_id` does have `ON DELETE CASCADE` and
/// enforcement is on for every pooled connection (src/db.rs), so this
/// currently happens via the cascade; the handler's explicit cleanup is a
/// backstop for a connection that might ever run with enforcement off, and
/// without either one those rows would be orphaned forever.
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
                .body(Body::empty())
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

/// When the user delete fails partway through, sessions must not be cleaned up
/// or logged as destroyed — only the user row deletion is attempted, so if it
/// fails, nothing changes. This regression test reproduces the bug: sessions
/// were deleted before the user row delete, so a failed delete left the user
/// alive but all their sessions orphaned.
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

    // Install a trigger that blocks deletion of the victim user
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
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // The delete should have failed. Database error → INTERNAL_SERVER_ERROR.
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

/// Pins that `delete_user` is correct whether `SQLite`'s cascade deletes the
/// sessions or the handler's explicit cleanup does. `setup_test_db` builds
/// its pool via `create_memory_pool` (`src/db.rs`), which is `max_size(1)`
/// — a single physical connection shared by every `pool.get()` call in this
/// test process, including the one the request handler will use inside
/// `spawn_blocking`. So enabling `PRAGMA foreign_keys = ON` on the
/// connection we grab here reaches the request's connection too; there is
/// no second connection for it to miss.
#[tokio::test]
async fn test_admin_delete_user_removes_sessions_even_with_foreign_keys_enforced() {
    let pool = common::setup_test_db();
    {
        let conn = pool.get().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
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
                .body(Body::empty())
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
        "victim's sessions should be gone whether the cascade or the explicit cleanup did it"
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

/// Distinct from `test_admin_delete_user_removes_their_sessions` above: that
/// test only proves sessions are gone by the time the handler responds,
/// which is true whether the handler's explicit cleanup or the DB cascade
/// did it. This test isolates the cascade itself — PRAGMA `foreign_keys=ON`
/// is now the default for every pooled connection (src/db.rs), so
/// `sessions.user_id REFERENCES users(id) ON DELETE CASCADE` fires on the
/// bare `DELETE FROM users` regardless of what the handler does afterwards.
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

    // Should return OK with error message (not redirect)
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("Invalid username or password"));
}

#[tokio::test]
async fn test_login_nonexistent_user() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    // Create a user so we don't get redirected to setup
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

    // Should clear the session cookie (max-age=0 or empty value)
    let set_cookie = response.headers().get(header::SET_COOKIE);
    assert!(set_cookie.is_some());
    let cookie_str = set_cookie.unwrap().to_str().unwrap();
    // Cookie should be cleared (either empty or max-age=0)
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

    // Byte-for-byte: unquoted directives make browsers silently ignore the
    // whole header, so a loose `contains` check wouldn't catch that mistake.
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

    // Should clear the session cookie (max-age=0 or empty value)
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
                .body(Body::from("username=admin&password=adminpass123"))
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
                .body(Body::from("username=&password=adminpass123"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Validation failure re-renders the setup form (200 OK).
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
    assert!(html.contains("Password must be at least 8 characters"));

    let user_repo = liftlog::repositories::UserRepository::new(pool);
    let count = user_repo.count().await.unwrap();
    assert_eq!(count, 0);
}

/// Pins the boundary itself rather than "some short password is rejected":
/// one character under the minimum must fail and exactly the minimum must
/// succeed, so an off-by-one in the comparison cannot pass.
#[tokio::test]
async fn test_setup_password_length_boundary() {
    for (password, should_create) in [("7chars!", false), ("8chars!!", true)] {
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
            password.len(),
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

    // Fresh session: last_touched_at is ~now, so within throttle.
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
    // Over the 90-day absolute cap even though expires_at (created as
    // now + 7d idle TTL) is still in the future.
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

// --- Fix 4: per-IP rate-limit bucketing at the HTTP level ---------------
//
// Nothing above ever attaches a `ConnectInfo`, so `login_submit` always sees
// `peer = None` and every request falls into the single "no peer" bucket.
// `oneshot` doesn't run the `into_make_service_with_connect_info` layer that
// does this in production, so these tests attach it manually via
// `common::with_peer` to actually exercise the per-IP dimension of
// `crate::net::client_ip` (Fixes 1a-1e) end to end.

/// Builds a login POST with a wrong password (so `release` never fires) and
/// the given extra headers.
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

    // Same (untrusted) peer, different forged X-Forwarded-For: must still
    // land in the same bucket, proving the header was ignored.
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

    // Different rightmost hop -> a different bucket -> still allowed.
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

    // Same rightmost hop as the first request, different leftmost -> same
    // bucket -> refused. This is what fails if the leftmost hop is ever
    // read instead of the rightmost.
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

/// Regression test for the confirmed bypass in Fix 1a: two separate
/// `X-Forwarded-For` header field lines must bucket by the *last* line, not
/// the first and not by ignoring the header entirely.
///
/// Three requests from the same peer, limit 1:
///   1. `["1.1.1.1", "198.51.100.7"]` -> 200
///   2. `["1.1.1.1", "198.51.100.8"]` -> 200 (different last line -> different bucket)
///   3. `["9.9.9.9", "198.51.100.7"]` -> 429 (same last line as #1, different first line -> same bucket)
///
/// If the header were ignored, all three would bucket by the shared peer and
/// #2 would be 429. If the *first* line were read instead of the last, #1
/// and #2 would share a bucket (`1.1.1.1`) and #2 would be 429. Either
/// regression fails this test.
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

/// Regression test for the confirmed bypass: with `LIFTLOG_TRUSTED_PROXY_HEADER`
/// unset (the default), a forged `X-Forwarded-For` must not mint a fresh
/// rate-limit bucket, even from a loopback peer that would have been
/// trusted had a header been configured.
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

    // Different forged X-Forwarded-For, same peer: must still land in the
    // same bucket, proving the header was never read because no header is
    // configured.
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

/// When `LIFTLOG_TRUSTED_PROXY_HEADER` selects `X-Forwarded-For`, `X-Real-IP` must
/// never be consulted, even when XFF is entirely absent from the request.
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

    // Different X-Real-IP, no XFF at all: must still land in the same
    // bucket (the peer), proving the non-selected header is never read.
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

// --- Fix 5: secure-cookie path exercised end to end ----------------------

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

    // And Set-Cookie should have been re-issued with a fresh Max-Age, under
    // whichever cookie name this deployment actually uses.
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

    // Exactly one Set-Cookie for the session cookie, and it must be the
    // removal, whichever cookie name this deployment uses.
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

/// End-to-end walk of the `LIFTLOG_COOKIE_SECURE=true` deployment path: login sets
/// the `__Host-session` cookie, an aged session slides and re-issues it, and
/// logout clears it. Guards against `SessionLayerState.cookie_secure` ever
/// getting lost: without it, a secure deployment would keep re-issuing a
/// plain `session=` cookie that `get_session_token` never reads, silently
/// logging active users out at the 7-day idle mark with nothing failing in
/// CI.
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

/// OWASP Session Management Cheat Sheet (Web Content Caching): authenticated
/// responses must tell browsers/proxies not to persist them, so the back
/// button on a shared device can't resurrect private content after logout.
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

/// Pins current behaviour: /auth/login holds no private content and is
/// reached with no valid session, so `sliding_session_middleware` never sets
/// `authenticated` and must not attach Cache-Control here. Guards against
/// silently widening the scope of the no-store change beyond authenticated
/// pages.
#[tokio::test]
async fn test_unauthenticated_login_page_has_no_cache_control() {
    let pool = common::setup_test_db();
    // A user must exist, otherwise /auth/login redirects to /auth/setup.
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

/// OWASP Session Management Cheat Sheet (Web Content Caching): the login
/// success response is the one place a *previously unauthenticated* request
/// transmits a fresh session identifier (via Set-Cookie), so
/// `sliding_session_middleware`'s request-scoped guard — which only stamps
/// Cache-Control when the incoming request already carried a valid session —
/// can never cover it. `login_submit` must set the headers itself.
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

/// Same rationale as `test_login_success_response_is_not_cacheable`, for the
/// first-user bootstrap path: `setup_submit`'s success response also mints
/// and transmits a fresh session identifier from an unauthenticated request.
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
                .body(Body::from("username=admin&password=adminpass123"))
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

/// HSTS is opt-in: a default-built app must never send
/// `Strict-Transport-Security`. Pins that the feature does not leak on by
/// accident.
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

/// When `LIFTLOG_HSTS_MAX_AGE` is configured (without subdomains), the header carries
/// exactly `max-age=<N>`.
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

/// When `LIFTLOG_HSTS_INCLUDE_SUBDOMAINS` is also configured, the header carries
/// `includeSubDomains` as well.
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

/// Layer-ordering regression guard: HSTS must be the *outermost* layer so it
/// still lands on a response that `csrf_origin_guard` rejects before the
/// request reaches session validation or a handler. If the HSTS layer were
/// registered inside the CSRF guard instead of outside it, this would fail.
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

/// The four baseline headers, unlike HSTS, are unconditional — no env var
/// gates them, so a deployment cannot end up without them by omission.
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

/// The clickjacking defence is worthless if it only ships on responses that
/// reach a handler. These two cover the paths that short-circuit inside a
/// middleware instead — the CSRF guard's 403 and the auth redirect's 302 —
/// which is what pins the layer's registration point.
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

/// The public share page is the one route a third party is meant to open, so
/// it is also the one most likely to be embedded — it must not be an exception.
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
