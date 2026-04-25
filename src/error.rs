use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Pool error: {0}")]
    Pool(#[from] r2d2::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized")]
    #[allow(dead_code)]
    Unauthorized,

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Validation error: {0}")]
    #[allow(dead_code)]
    Validation(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Password hash error")]
    PasswordHash,
}

impl From<askama::Error> for AppError {
    fn from(err: askama::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(err: tokio::task::JoinError) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Database(e) => {
                tracing::error!("Database error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            }
            AppError::Pool(e) => {
                tracing::error!("Pool error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal error".to_string(),
                )
            }
            AppError::PasswordHash => {
                tracing::error!("Password hash error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal error".to_string(),
                )
            }
        };

        (status, message).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn askama_error_converts_to_internal() {
        let err: AppError = askama::Error::Fmt.into();
        assert!(matches!(err, AppError::Internal(_)));
    }

    #[tokio::test]
    async fn join_error_converts_to_internal() {
        let handle: tokio::task::JoinHandle<()> = tokio::task::spawn(async {
            panic!("intentional panic for test");
        });
        let join_err = handle.await.expect_err("join must fail after panic");
        let app_err: AppError = join_err.into();
        assert!(matches!(app_err, AppError::Internal(_)));
    }
}
