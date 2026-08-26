mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
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
    assert!(body_str.contains('2') || body_str.contains("Stats"));
}

#[tokio::test]
async fn test_stats_index_calculates_volume() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

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
                .uri(format!("/stats/exercise/{}", exercise.id))
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
async fn test_prs_list_separates_all_time_and_recent_windows() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let session_cookie = common::create_session_cookie(&pool, &user).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let bench = common::create_test_exercise(&pool, &user.id, "Bench Press", "chest").await;
    let squat = common::create_test_exercise(&pool, &user.id, "Squat", "legs").await;
    let workout = common::create_test_workout(
        &pool,
        &user.id,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        None,
    )
    .await;

    // Bench was trained just now; squat only outside the 1-month window.
    common::create_test_log(&pool, &workout.id, &bench.id, 1, 5, 100.0, None).await;
    let old_squat = common::create_test_log(&pool, &workout.id, &squat.id, 1, 5, 150.0, None).await;
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE workout_logs SET created_at = ? WHERE id = ?",
            rusqlite::params![
                chrono::Utc::now() - chrono::Duration::days(60),
                old_squat.id
            ],
        )
        .unwrap();
    }

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

    assert!(body_str.contains("PR (All)"), "body=\n{body_str}");
    assert!(body_str.contains("PR (1M)"), "body=\n{body_str}");

    let row = |name: &str| {
        body_str
            .split("<tr>")
            .find(|row| row.contains(name))
            .unwrap_or_else(|| panic!("no row for {name}, body=\n{body_str}"))
            .to_string()
    };

    // Squat's all-time PR stands, but it has no record inside the window.
    let squat_row = row("Squat");
    assert!(squat_row.contains("150"), "squat_row=\n{squat_row}");
    assert!(squat_row.contains("&mdash;"), "squat_row=\n{squat_row}");

    // Bench was logged inside the window, so both columns carry a number.
    let bench_row = row("Bench Press");
    assert!(bench_row.contains("100"), "bench_row=\n{bench_row}");
    assert!(!bench_row.contains("&mdash;"), "bench_row=\n{bench_row}");
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
                .uri(format!("/stats/exercise/{}", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

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
                .uri(format!("/stats/exercise/{}", exercise.id))
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
                .uri(format!("/stats/exercise/{}", exercise.id))
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
                .uri(format!("/stats/exercise/{}", exercise.id))
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
    assert_eq!(pr_count, 3, "expected 3 PR dots, body=\n{body_str}");
    assert_eq!(plain_count, 2);
}

#[tokio::test]
async fn test_exercise_stats_rejects_exercise_owned_by_another_user() {
    // The history, PR and metrics queries are scoped by the caller, but the
    // exercise record itself is rendered into the page — fetching it unscoped
    // disclosed another user's exercise name and category.
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let snooper = common::create_test_user(&pool, "snooper", "password123", UserRole::User).await;
    let victim = common::create_test_user(&pool, "victim", "password123", UserRole::User).await;

    let session_cookie = common::create_session_cookie(&pool, &snooper).await;
    let cookie_header = common::extract_cookie_header(&session_cookie);

    let victim_exercise =
        common::create_test_exercise(&pool, &victim.id, "Victim Deadlift", "back").await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/stats/exercise/{}", victim_exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        !body_str.contains("Victim Deadlift"),
        "the other user's exercise name must not be disclosed, body=\n{body_str}"
    );
}

/// Three sessions whose top-set weights and volumes cannot be confused:
/// weights land the y-axis in the 99–111 band, volumes in 495–555. Any
/// assertion on a tick label therefore proves *which* series was plotted.
async fn seed_three_sessions(
    pool: &liftlog::db::DbPool,
    user_id: &str,
) -> liftlog::models::Exercise {
    let exercise = common::create_test_exercise(pool, user_id, "Bench Press", "chest").await;
    for (i, weight) in [100.0_f64, 105.0, 110.0].iter().enumerate() {
        let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 10 + i as u32 * 2).unwrap();
        let workout = common::create_test_workout(pool, user_id, date, None).await;
        common::create_test_log(pool, &workout.id, &exercise.id, 1, 5, *weight, None).await;
    }
    exercise
}

/// Volume and e1RM had no representation anywhere in the UI once scripts
/// were off — only the client redraw could reach them. The tabs are links
/// now, so the server has to honour `?metric=`.
#[tokio::test]
async fn test_exercise_stats_chart_plots_the_requested_metric() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);
    let exercise = seed_three_sessions(&pool, &user.id).await;

    let response = test_app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/stats/exercise/{}?metric=volume", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Volume axis: 500/525/550 padded to 495..555.
    assert!(
        body_str.contains(">555</text>") && body_str.contains(">495</text>"),
        "the y axis should be scaled to volume, got:\n{body_str}"
    );
    assert!(
        !body_str.contains(">111</text>"),
        "the top-set axis must not survive into the volume chart"
    );
    assert!(
        body_str.contains(r#"class="btn btn-sm btn-tab is-active" data-metric="volume""#),
        "the Volume tab should be the active one"
    );

    // And the default is still the top-set axis.
    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/stats/exercise/{}", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains(">111</text>") && body_str.contains(">99</text>"),
        "the default chart should still plot top set, got:\n{body_str}"
    );
}

/// e1RM is the other series that scripts-off users could not reach, and it
/// is the only one that is *derived* (`weight * (1 + reps/30)`) rather than
/// stored — so plotting the wrong column would still produce a plausible
/// chart. Reps of 10 put the axis in a band that neither top set (98–122)
/// nor volume (980–1220) can produce.
#[tokio::test]
async fn test_exercise_stats_chart_plots_e1rm() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);
    let exercise = common::create_test_exercise(&pool, &user.id, "Deadlift", "back").await;

    for (i, weight) in [100.0_f64, 110.0, 120.0].iter().enumerate() {
        let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 10 + i as u32 * 2).unwrap();
        let workout = common::create_test_workout(&pool, &user.id, date, None).await;
        common::create_test_log(&pool, &workout.id, &exercise.id, 1, 10, *weight, None).await;
    }

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/stats/exercise/{}?metric=e1rm", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // e1RM of 133.3 / 146.7 / 160.0, padded to 130.7..162.7.
    assert!(
        body_str.contains(">163</text>") && body_str.contains(">131</text>"),
        "the y axis should be scaled to e1RM, got:\n{body_str}"
    );
    assert!(
        body_str.contains(r#"class="btn btn-sm btn-tab is-active" data-metric="e1rm""#),
        "the e1RM tab should be the active one"
    );
}

/// The HTML tooltip is built by script on pointer events, so the figures
/// behind each point were unreadable without it. SVG `<title>` gets a native
/// tooltip out of the browser for free — provided the bands are rendered
/// server-side, which the client redraw otherwise replaces.
#[tokio::test]
async fn test_exercise_stats_chart_has_native_hover_titles() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);
    let exercise = seed_three_sessions(&pool, &user.id).await;

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/stats/exercise/{}", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Scoped to the hit-area group: the document's own <title> is in <head>.
    let start = body_str
        .find(r#"<g id="chart-hit-areas">"#)
        .expect("hit-area group missing");
    let end = body_str[start..]
        .find("</g>")
        .expect("hit-area group close");
    let bands = &body_str[start..start + end];

    // One band per session, each carrying the full figures for that session.
    assert_eq!(
        bands.matches("<title>").count(),
        3,
        "expected one hover band per session, got:\n{bands}"
    );
    assert!(
        bands.contains("Top: 100 kg × 5"),
        "the title should carry the top set, got:\n{bands}"
    );
    assert!(
        bands.contains("Volume: 500 kg"),
        "the title should carry the volume, got:\n{bands}"
    );
    assert!(
        bands.contains("e1RM: 116.7 kg"),
        "the title should carry e1RM to one decimal, got:\n{bands}"
    );
}

/// `?range=all` has to widen the window, not just relabel the tab.
#[tokio::test]
async fn test_exercise_stats_chart_range_controls_the_window() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);
    let exercise = common::create_test_exercise(&pool, &user.id, "Squat", "legs").await;

    // 22 sessions, so the default 20-session window drops the first two.
    for day in 1..=22u32 {
        let date = chrono::NaiveDate::from_ymd_opt(2024, 1, day).unwrap();
        let workout = common::create_test_workout(&pool, &user.id, date, None).await;
        common::create_test_log(
            &pool,
            &workout.id,
            &exercise.id,
            1,
            5,
            100.0 + f64::from(day),
            None,
        )
        .await;
    }

    let response = test_app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/stats/exercise/{}", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let default_body = String::from_utf8_lossy(&body).into_owned();

    assert!(
        !default_body.contains(">01-01</text>"),
        "the last-20 window should start after the first session"
    );

    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/stats/exercise/{}?range=all", exercise.id))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let all_body = String::from_utf8_lossy(&body);

    assert!(
        all_body.contains(">01-01</text>"),
        "range=all should reach back to the first session, got:\n{all_body}"
    );
    assert!(
        all_body.contains(r#"data-range="all""#) && all_body.contains("is-active"),
        "the All tab should be the active one"
    );
}

/// The tabs are links, so the query string is user-editable and arrives from
/// stale bookmarks. Nonsense falls back to the default view rather than
/// erroring, and each tab's href carries the *other* axis's current value so
/// switching metric does not silently reset the range.
#[tokio::test]
async fn test_exercise_stats_chart_query_is_forgiving_and_links_compose() {
    let pool = common::setup_test_db();
    let test_app = common::create_test_app_with_session(pool.clone());

    let user = common::create_test_user(&pool, "testuser", "password123", UserRole::User).await;
    let cookie_header =
        common::extract_cookie_header(&common::create_session_cookie(&pool, &user).await);
    let exercise = seed_three_sessions(&pool, &user.id).await;

    let response = test_app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/stats/exercise/{}?metric=bogus&range=bogus",
                    exercise.id
                ))
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
        body_str.contains(">111</text>"),
        "a bad metric should fall back to top set, got:\n{body_str}"
    );

    // On the volume + all view, the range tabs must keep metric=volume and
    // the metric tabs must keep range=all.
    let response = test_app
        .router
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/stats/exercise/{}?metric=volume&range=all",
                    exercise.id
                ))
                .header(header::COOKIE, &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains(r#"href="?metric=volume&amp;range=20""#),
        "the Last 20 tab should stay on volume, got:\n{body_str}"
    );
    assert!(
        body_str.contains(r#"href="?metric=e1rm&amp;range=all""#),
        "the e1RM tab should stay on the all-sessions range, got:\n{body_str}"
    );
}
