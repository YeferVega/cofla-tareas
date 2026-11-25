use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::json::json_response;

#[derive(Debug)]
pub enum AppError {
    Unavailable,
    RouteNotFound,
    TaskNotFound,
    InvalidId,
    PayloadTooLarge,
    UnsupportedMediaType,
    BadRequest(&'static str),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            AppError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            AppError::RouteNotFound | AppError::TaskNotFound => StatusCode::NOT_FOUND,
            AppError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            AppError::InvalidId | AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
        }
    }

    fn message(&self) -> &str {
        match self {
            AppError::Unavailable => "Service unavailable",
            AppError::RouteNotFound => "Not found",
            AppError::TaskNotFound => "Task not found",
            AppError::InvalidId => "Invalid ID",
            AppError::PayloadTooLarge => "Request too large",
            AppError::UnsupportedMediaType => "Send application/json",
            AppError::BadRequest(message) => message,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        json_response(self.status(), Json(json!({ "error": self.message() })))
    }
}
