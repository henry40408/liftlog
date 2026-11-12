use axum::{
    http::{HeaderValue, StatusCode, header},
    response::IntoResponse,
};

const APP_CSS: &str = include_str!("../../assets/app.css");
const APP_JS: &str = include_str!("../../assets/app.js");
/// The pages request these with `?v=<GIT_VERSION>`, so a new build gets a new URL.
const CACHE_CONTROL: &str = "public, max-age=86400";

fn asset(content_type: &'static str, body: &'static str) -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static(CACHE_CONTROL),
            ),
        ],
        body,
    )
}

pub async fn app_css() -> impl IntoResponse {
    asset("text/css; charset=utf-8", APP_CSS)
}

pub async fn app_js() -> impl IntoResponse {
    asset("text/javascript; charset=utf-8", APP_JS)
}
