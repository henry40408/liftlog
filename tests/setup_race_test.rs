use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

mod common;

const CONCURRENT_SETUPS: usize = 8;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_setup_creates_exactly_one_admin() {
    let db = common::FileTestDb::new();
    let router = common::create_test_app(db.pool.clone());

    let handles: Vec<_> = (0..CONCURRENT_SETUPS)
        .map(|i| {
            let router = router.clone();
            tokio::spawn(async move {
                let request = Request::builder()
                    .method("POST")
                    .uri("/auth/setup")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!(
                        "username=admin{i}&password={}&setup_token={}",
                        common::STRONG_PASSWORD,
                        common::TEST_SETUP_TOKEN
                    )))
                    .unwrap();
                let response = router.oneshot(request).await.unwrap();
                let location = response
                    .headers()
                    .get(header::LOCATION)
                    .map(|v| v.to_str().unwrap().to_string());
                (response.status(), location)
            })
        })
        .collect();

    let mut winners = 0;
    for handle in handles {
        let (status, location) = handle.await.unwrap();
        assert_eq!(status, StatusCode::SEE_OTHER, "no request may fail");
        match location.as_deref() {
            Some("/") => winners += 1,
            Some("/auth/login") => {}
            other => panic!("unexpected redirect: {other:?}"),
        }
    }
    assert_eq!(winners, 1, "exactly one setup request may win");

    let conn = db.pool.get().unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
