use axum::body::to_bytes;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use cofla_tareas_backend::app::build_router;
use cofla_tareas_backend::state::AppState;

#[tokio::test]
async fn health_memory_returns_200_with_ok_storage_and_version() {
    let app = build_router(AppState::memory("1.0"));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(CACHE_CONTROL).unwrap(),
        "no-store",
        "health JSON must not be cached"
    );
    assert_eq!(
        response.headers().get(CONTENT_TYPE).unwrap(),
        "application/json; charset=utf-8"
    );

    let bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        body,
        json!({ "ok": true, "storage": "memory", "version": "1.0" })
    );
}

#[tokio::test]
async fn health_postgres_returns_503_when_database_unreachable() {
    let app = build_router(
        AppState::postgres("1.0", "postgres://postgres@127.0.0.1:9/postgres").unwrap(),
    );
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");

    let bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body, json!({ "error": "Service unavailable" }));
}
