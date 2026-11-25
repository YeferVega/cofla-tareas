use std::sync::Arc;
use std::time::Duration;

use axum::body::to_bytes;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use serial_test::serial;
use sqlx::Executor;
use tower::ServiceExt;

use cofla_tareas_backend::app::build_router;
use cofla_tareas_backend::state::{AppState, StorageMode};
use cofla_tareas_backend::storage::{PostgresStore, TaskStore};

const UNREACHABLE_DB_URL: &str = "postgres://postgres@127.0.0.1:9/postgres";

fn db_url() -> Option<String> {
    std::env::var("TEST_DATABASE_URL").ok()
}

async fn setup_live_store(url: &str) -> Arc<PostgresStore> {
    let store = Arc::new(
        PostgresStore::connect_with_retry(url, 10, Duration::from_millis(500))
            .await
            .expect("postgres test database should be reachable"),
    );
    store
        .pool()
        .execute("TRUNCATE TABLE tasks RESTART IDENTITY")
        .await
        .expect("truncate should succeed");
    store
}

fn app_with_store(mode: StorageMode, store: Arc<dyn TaskStore>) -> Router {
    build_router(AppState::with_store(mode, "1.0", store))
}

async fn send(
    app: Router,
    method: Method,
    uri: &str,
    content_type: Option<&str>,
    body: &str,
) -> (StatusCode, HeaderMap, Value) {
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
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    (
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn post_task(app: &Router, title: &str) -> (StatusCode, Value) {
    let payload = json!({ "title": title }).to_string();
    let (status, _headers, body) = send(
        app.clone(),
        Method::POST,
        "/api/tasks",
        Some("application/json"),
        &payload,
    )
    .await;
    (status, body)
}

#[tokio::test]
#[serial]
async fn health_reports_postgres_storage_when_database_answers() {
    let Some(url) = db_url() else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let store = setup_live_store(&url).await;
    let app = app_with_store(StorageMode::Postgres, store);

    let (status, _headers, body) = send(app, Method::GET, "/health", None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "ok": true, "storage": "postgres", "version": "1.0" })
    );
}

#[tokio::test]
#[serial]
async fn postgres_crud_matches_memory_results() {
    let Some(url) = db_url() else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let store = setup_live_store(&url).await;
    let app = app_with_store(StorageMode::Postgres, store);

    let (status, _headers, body) = send(app.clone(), Method::GET, "/api/tasks", None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));

    let (status, created) = post_task(&app, "  padded title  ").await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["title"], "padded title");
    assert_eq!(created["id"], 1);
    assert_eq!(created["done"], false);

    let (status, patched) = {
        let (status, _h, body) = send(
            app.clone(),
            Method::PATCH,
            "/api/tasks/1",
            Some("application/json"),
            r#"{"done":true,"title":"ignored"}"#,
        )
        .await;
        (status, body)
    };
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        patched,
        json!({ "id": 1, "title": "padded title", "done": true })
    );

    post_task(&app, "second").await;
    let (_status, _headers, listed) = send(app.clone(), Method::GET, "/api/tasks", None, "").await;
    assert_eq!(
        listed,
        json!([
            { "id": 1, "title": "padded title", "done": true },
            { "id": 2, "title": "second", "done": false },
        ])
    );

    let (status, _headers, body) =
        send(app.clone(), Method::DELETE, "/api/tasks/1", None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "ok": true }));

    let (status, _headers, body) =
        send(app.clone(), Method::DELETE, "/api/tasks/1", None, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "Task not found" }));
}

#[tokio::test]
#[serial]
async fn postgres_validates_title_length_boundaries() {
    let Some(url) = db_url() else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let store = setup_live_store(&url).await;
    let app = app_with_store(StorageMode::Postgres, store);

    let (status, _body) = post_task(&app, &"x".repeat(200)).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = post_task(&app, &"y".repeat(201)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "Task title must be between 1 and 200 characters"
    );

    let (status, body) = post_task(&app, "   ").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "Task title must be between 1 and 200 characters"
    );
}

#[tokio::test]
#[serial]
async fn postgres_returns_404_for_unknown_task_id() {
    let Some(url) = db_url() else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let store = setup_live_store(&url).await;
    let app = app_with_store(StorageMode::Postgres, store);

    let (status, _headers, body) = send(
        app.clone(),
        Method::PATCH,
        "/api/tasks/999",
        Some("application/json"),
        r#"{"done":true}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "Task not found" }));

    let (status, _headers, body) =
        send(app.clone(), Method::DELETE, "/api/tasks/999", None, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "Task not found" }));
}

#[tokio::test]
#[serial]
async fn responses_return_503_once_the_database_connection_drops() {
    let Some(url) = db_url() else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let store = setup_live_store(&url).await;
    let app = app_with_store(StorageMode::Postgres, store.clone());

    let (status, _body) = post_task(&app, "before drop").await;
    assert_eq!(status, StatusCode::CREATED);

    store.pool().close().await;

    let (status, _headers, body) = send(app.clone(), Method::GET, "/api/tasks", None, "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, json!({ "error": "Service unavailable" }));

    let (status, _headers, body) = send(app.clone(), Method::GET, "/health", None, "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, json!({ "error": "Service unavailable" }));
}

#[tokio::test]
async fn connect_with_retry_gives_up_after_configured_attempts_on_unreachable_host() {
    let started = std::time::Instant::now();
    let result =
        PostgresStore::connect_with_retry(UNREACHABLE_DB_URL, 2, Duration::from_millis(50)).await;
    let elapsed = started.elapsed();
    assert!(
        result.is_err(),
        "unreachable database must fail after attempts"
    );
    assert!(
        elapsed >= Duration::from_millis(50),
        "delay between attempts must elapse"
    );
    assert!(
        elapsed < Duration::from_secs(15),
        "retry loop must stop at configured attempts"
    );
}
