use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use serde_json::json;

use crate::error::AppError;
use crate::json::json_response;
use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Result<Response, AppError> {
    state
        .store
        .ping()
        .await
        .map_err(|_| AppError::Unavailable)?;
    Ok(json_response(
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "storage": state.mode_name(),
            "version": state.version,
        })),
    ))
}
