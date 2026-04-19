use axum::{
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
};

const FAVICON_SVG: &[u8] = include_bytes!("../../assets/favicon.svg");
const APPLE_TOUCH_ICON_PNG: &[u8] = include_bytes!("../../assets/apple-touch-icon.png");
const CACHE_CONTROL: &str = "public, max-age=86400";

pub async fn favicon_svg() -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("image/svg+xml"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static(CACHE_CONTROL),
            ),
        ],
        FAVICON_SVG,
    )
}

pub async fn apple_touch_icon() -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("image/png")),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static(CACHE_CONTROL),
            ),
        ],
        APPLE_TOUCH_ICON_PNG,
    )
}
