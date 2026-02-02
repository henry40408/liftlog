use axum::{http::StatusCode, response::IntoResponse};
use liftlog::error::AppError;

#[test]
fn test_not_found_returns_404() {
    let error = AppError::NotFound("Resource not found".to_string());
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_forbidden_returns_403() {
    let error = AppError::Forbidden("Access denied".to_string());
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn test_bad_request_returns_400() {
    let error = AppError::BadRequest("Invalid input".to_string());
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_unauthorized_returns_401() {
    let error = AppError::Unauthorized;
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_internal_returns_500() {
    let error = AppError::Internal("Something went wrong".to_string());
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_validation_returns_400() {
    let error = AppError::Validation("Invalid field".to_string());
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_password_hash_returns_500() {
    let error = AppError::PasswordHash;
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
