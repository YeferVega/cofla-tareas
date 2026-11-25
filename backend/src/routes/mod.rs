pub mod health;
pub mod tasks;

use axum::response::{IntoResponse, Response};

use crate::error::AppError;

pub async fn not_found() -> Response {
    AppError::RouteNotFound.into_response()
}
