use axum::body::to_bytes;
use axum::extract::{Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::Json;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::json::json_response;
use crate::state::AppState;

const MAX_BODY_BYTES: usize = 8192;
const TITLE_ERROR: &str = "Task title must be between 1 and 200 characters";
const TASKS_PREFIX: &str = "/api/tasks/";

pub async fn list(State(state): State<AppState>) -> Result<Response, AppError> {
    let tasks = state
        .store
        .list()
        .await
        .map_err(|_| AppError::Unavailable)?;
    Ok(json_response(
        StatusCode::OK,
        Json(serde_json::to_value(tasks).unwrap()),
    ))
}

pub async fn create(State(state): State<AppState>, request: Request) -> Result<Response, AppError> {
    let (parts, body) = request.into_parts();
    require_json_content_type(&parts.headers)?;
    let value = read_json(body).await?;
    let raw_title = value
        .get("title")
        .and_then(Value::as_str)
        .ok_or(AppError::BadRequest(TITLE_ERROR))?;
    let title = raw_title.trim();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(AppError::BadRequest(TITLE_ERROR));
    }
    let task = state
        .store
        .create(title.to_string())
        .await
        .map_err(|_| AppError::Unavailable)?;
    Ok(json_response(
        StatusCode::CREATED,
        Json(serde_json::to_value(task).unwrap()),
    ))
}

pub async fn patch(State(state): State<AppState>, request: Request) -> Result<Response, AppError> {
    let (parts, body) = request.into_parts();
    let id = parse_id(parts.uri.path())?;
    require_json_content_type(&parts.headers)?;
    let value = read_json(body).await?;
    let done = value
        .get("done")
        .and_then(Value::as_bool)
        .ok_or(AppError::BadRequest("done must be a boolean"))?;
    let task = state
        .store
        .set_done(id, done)
        .await
        .map_err(|_| AppError::Unavailable)?
        .ok_or(AppError::TaskNotFound)?;
    Ok(json_response(
        StatusCode::OK,
        Json(serde_json::to_value(task).unwrap()),
    ))
}

pub async fn delete(State(state): State<AppState>, request: Request) -> Result<Response, AppError> {
    let (parts, _body) = request.into_parts();
    let id = parse_id(parts.uri.path())?;
    if state
        .store
        .remove(id)
        .await
        .map_err(|_| AppError::Unavailable)?
    {
        return Ok(json_response(StatusCode::OK, Json(json!({ "ok": true }))));
    }
    Err(AppError::TaskNotFound)
}

fn require_json_content_type(headers: &HeaderMap) -> Result<(), AppError> {
    let is_json = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if is_json {
        Ok(())
    } else {
        Err(AppError::UnsupportedMediaType)
    }
}

async fn read_json(body: axum::body::Body) -> Result<Value, AppError> {
    let bytes = to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| AppError::PayloadTooLarge)?;
    serde_json::from_slice(&bytes).map_err(|_| AppError::BadRequest("Invalid JSON"))
}

fn parse_id(path: &str) -> Result<i32, AppError> {
    let raw = path
        .strip_prefix(TASKS_PREFIX)
        .ok_or(AppError::RouteNotFound)?;
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AppError::RouteNotFound);
    }
    let value = raw.parse::<i64>().map_err(|_| AppError::InvalidId)?;
    if !(1..=2147483647).contains(&value) {
        return Err(AppError::InvalidId);
    }
    Ok(value as i32)
}
