use axum::body::to_bytes;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderValue, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use cofla_tareas_backend::app::build_router;
use cofla_tareas_backend::state::AppState;

fn app() -> Router {
    build_router(AppState::memory("1.0"))
}

async fn call(
    app: Router,
    method: Method,
    uri: &str,
    content_type: Option<&str>,
    body: &str,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(value) = content_type {
        builder = builder.header(CONTENT_TYPE, HeaderValue::from_str(value).unwrap());
    }
    let response = app
        .oneshot(
            builder
                .body(axum::body::Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn get_json(app: &Router, uri: &str) -> (StatusCode, Value) {
    call(app.clone(), Method::GET, uri, None, "").await
}

async fn post_json(app: &Router, body: &str) -> (StatusCode, Value) {
    call(
        app.clone(),
        Method::POST,
        "/api/tasks",
        Some("application/json"),
        body,
    )
    .await
}

async fn patch_json(app: &Router, uri: &str, body: &str) -> (StatusCode, Value) {
    call(
        app.clone(),
        Method::PATCH,
        uri,
        Some("application/json"),
        body,
    )
    .await
}

async fn delete_task(app: &Router, uri: &str) -> (StatusCode, Value) {
    call(app.clone(), Method::DELETE, uri, None, "").await
}

#[tokio::test]
async fn list_returns_empty_array_then_created_tasks_in_id_order() {
    let app = app();
    let (status, body) = get_json(&app, "/api/tasks").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));

    post_json(&app, r#"{"title":"first"}"#).await;
    post_json(&app, r#"{"title":"second"}"#).await;

    let (status, body) = get_json(&app, "/api/tasks").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!([
            { "id": 1, "title": "first", "done": false },
            { "id": 2, "title": "second", "done": false },
        ])
    );
}

#[tokio::test]
async fn create_returns_201_with_task_shape() {
    let app = app();
    let (status, body) = post_json(&app, r#"{"title":"write spec"}"#).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        body,
        json!({ "id": 1, "title": "write spec", "done": false })
    );
}

#[tokio::test]
async fn create_trims_surrounding_whitespace_from_title() {
    let app = app();
    let (status, body) = post_json(&app, r#"{"title":"  padded  "}"#).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["title"], "padded");
}

#[tokio::test]
async fn create_rejects_missing_content_type_with_415() {
    let app = app();
    let (status, body) = call(
        app.clone(),
        Method::POST,
        "/api/tasks",
        None,
        r#"{"title":"ok"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn create_rejects_non_json_content_type_with_415() {
    let app = app();
    let (status, body) = call(
        app.clone(),
        Method::POST,
        "/api/tasks",
        Some("text/plain"),
        r#"{"title":"ok"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn create_rejects_malformed_json_with_400() {
    let app = app();
    let (status, body) = post_json(&app, r#"{"title": oops}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn create_rejects_blank_title_with_400() {
    let app = app();
    let (status, body) = post_json(&app, r#"{"title":"   "}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "Task title must be between 1 and 200 characters"
    );

    let (status, _) = post_json(&app, r#"{"nope":"x"}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = post_json(&app, r#"{"title":123}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_accepts_200_char_title_but_rejects_201_char_title() {
    let app = app();
    let ok_title = "x".repeat(200);
    let (status, _) = post_json(&app, &json!({ "title": ok_title }).to_string()).await;
    assert_eq!(status, StatusCode::CREATED);

    let too_long = "x".repeat(201);
    let (status, body) = post_json(&app, &json!({ "title": too_long }).to_string()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "Task title must be between 1 and 200 characters"
    );
}

#[tokio::test]
async fn create_rejects_body_over_8192_bytes_with_413() {
    let app = app();
    let huge_title = "x".repeat(9000);
    let (status, body) = post_json(&app, &json!({ "title": huge_title }).to_string()).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn patch_toggles_done_and_keeps_title() {
    let app = app();
    post_json(&app, r#"{"title":"original"}"#).await;

    let (status, body) = patch_json(&app, "/api/tasks/1", r#"{"done":true}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "id": 1, "title": "original", "done": true }));

    let (status, body) =
        patch_json(&app, "/api/tasks/1", r#"{"done":false,"title":"ignored"}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "id": 1, "title": "original", "done": false }));
}

#[tokio::test]
async fn patch_rejects_non_boolean_done_with_400() {
    let app = app();
    post_json(&app, r#"{"title":"t"}"#).await;

    let (status, body) = patch_json(&app, "/api/tasks/1", r#"{"done":"yes"}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "done must be a boolean");

    let (status, _) = patch_json(&app, "/api/tasks/1", "{}").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn patch_returns_404_for_unknown_id() {
    let app = app();
    let (status, body) = patch_json(&app, "/api/tasks/99", r#"{"done":true}"#).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "Task not found");
}

#[tokio::test]
async fn patch_validates_id_range() {
    let app = app();
    let (status, body) = patch_json(&app, "/api/tasks/0", r#"{"done":true}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid ID");

    let (status, _) = patch_json(&app, "/api/tasks/2147483648", r#"{"done":true}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = patch_json(
        &app,
        "/api/tasks/99999999999999999999999",
        r#"{"done":true}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn patch_returns_404_for_non_numeric_id_like_baseline_regex() {
    let app = app();
    let (status, _) = patch_json(&app, "/api/tasks/abc", r#"{"done":true}"#).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = patch_json(&app, "/api/tasks/-5", r#"{"done":true}"#).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_returns_200_ok_then_404_when_deleted_twice() {
    let app = app();
    post_json(&app, r#"{"title":"remove me"}"#).await;

    let (status, body) = delete_task(&app, "/api/tasks/1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "ok": true }));

    let (status, body) = delete_task(&app, "/api/tasks/1").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "Task not found");
}

#[tokio::test]
async fn delete_returns_404_for_unknown_id() {
    let app = app();
    let (status, body) = delete_task(&app, "/api/tasks/1").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "Task not found");
}

#[tokio::test]
async fn delete_validates_id_range() {
    let app = app();
    let (status, body) = delete_task(&app, "/api/tasks/0").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid ID");

    let (status, _) = delete_task(&app, "/api/tasks/2147483648").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
