use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use sqlx::Error as SqlxError;
use tera::Error as TeraError;

#[derive(Debug)]
pub enum AppError {
    SqlxError(SqlxError),
    TeraError(TeraError),
    ItemNotFound,
    BadRequest(String),
    InternalServerError(String),
}

impl From<SqlxError> for AppError {
    fn from(err: SqlxError) -> Self {
        match err {
            SqlxError::RowNotFound => AppError::ItemNotFound,
            _ => AppError::SqlxError(err),
        }
    }
}

impl From<TeraError> for AppError {
    fn from(err: TeraError) -> Self {
        AppError::TeraError(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::SqlxError(e) => {
                tracing::error!("SQLx error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            }
            AppError::TeraError(e) => {
                tracing::error!("Tera error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Template rendering error".to_string(),
                )
            }
            AppError::ItemNotFound => (StatusCode::NOT_FOUND, "Item not found".to_string()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::InternalServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(json!({ "error": error_message }));
        (status, body).into_response()
    }
}
