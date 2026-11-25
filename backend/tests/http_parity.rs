use axum::body::to_bytes;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use cofla_tareas_backend::app::build_router;
use cofla_tareas_backend::state::AppState;

const CSP: &str = "default-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'self'";
const UNREACHABLE_DB_URL: &str = "postgres://postgres@127.0.0.1:9/postgres";

fn app() -> Router {
    build_router(AppState::memory("1.0"))
}

fn postgres_app() -> Router {
    build_router(AppState::postgres("1.0", UNREACHABLE_DB_URL).unwrap())
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

fn assert_security_headers(headers: &HeaderMap) {
    assert_eq!(
        headers.get(CONTENT_SECURITY_POLICY),
        Some(&HeaderValue::from_static(CSP)),
        "exact baseline CSP must be present"
    );
    assert_eq!(
        headers.get(X_CONTENT_TYPE_OPTIONS),
        Some(&HeaderValue::from_static("nosniff"))
    );
    assert_eq!(
        headers.get(CACHE_CONTROL),
        Some(&HeaderValue::from_static("no-store")),
        "JSON responses must not be cached"
    );
}

#[tokio::test]
async fn health_success_carries_security_headers() {
    let (status, headers, body) = send(app(), Method::GET, "/health", None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "ok": true, "storage": "memory", "version": "1.0" })
    );
    assert_security_headers(&headers);
}

#[tokio::test]
async fn task_crud_success_responses_carry_security_headers() {
    let app = app();

    let (status, headers, body) = send(app.clone(), Method::GET, "/api/tasks", None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
    assert_security_headers(&headers);

    let (status, headers, _) = send(
        app.clone(),
        Method::POST,
        "/api/tasks",
        Some("application/json"),
        r#"{"title":"header check"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_security_headers(&headers);

    let (status, headers, _) = send(
        app.clone(),
        Method::PATCH,
        "/api/tasks/1",
        Some("application/json"),
        r#"{"done":true}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_security_headers(&headers);

    let (status, headers, body) = send(app.clone(), Method::DELETE, "/api/tasks/1", None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "ok": true }));
    assert_security_headers(&headers);
}

#[tokio::test]
async fn client_error_responses_carry_security_headers() {
    let app = app();

    let (status, headers, _) = send(
        app.clone(),
        Method::POST,
        "/api/tasks",
        Some("application/json"),
        r#"{"title":" "}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_security_headers(&headers);

    let (status, headers, _) = send(
        app.clone(),
        Method::POST,
        "/api/tasks",
        Some("text/plain"),
        r#"{"title":"x"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_security_headers(&headers);

    let (status, headers, _) = send(app.clone(), Method::DELETE, "/api/tasks/42", None, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_security_headers(&headers);
}

#[tokio::test]
async fn unreachable_database_health_503_carries_security_headers() {
    let (status, headers, body) = send(postgres_app(), Method::GET, "/health", None, "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, json!({ "error": "Service unavailable" }));
    assert_security_headers(&headers);
}

#[tokio::test]
async fn put_on_tasks_collection_returns_404_instead_of_405() {
    let (status, headers, body) = send(
        app(),
        Method::PUT,
        "/api/tasks",
        Some("application/json"),
        r#"{"title":"x"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "Not found" }));
    assert_security_headers(&headers);
}

#[tokio::test]
async fn post_on_health_returns_404_instead_of_405() {
    let (status, headers, body) = send(
        app(),
        Method::POST,
        "/health",
        Some("application/json"),
        "{}",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "Not found" }));
    assert_security_headers(&headers);
}

#[tokio::test]
async fn get_on_task_id_returns_404_because_baseline_has_no_get_by_id() {
    let (status, body) = {
        let (status, _headers, body) = send(app(), Method::GET, "/api/tasks/1", None, "").await;
        (status, body)
    };
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "Not found" }));
}

#[tokio::test]
async fn unknown_routes_return_404_json_with_security_headers() {
    for uri in ["/nope", "/api", "/api/tasks/1/extra", "/healthz"] {
        let (status, headers, body) = send(app(), Method::GET, uri, None, "").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "unexpected status for {uri}");
        assert_eq!(
            body,
            json!({ "error": "Not found" }),
            "unexpected body for {uri}"
        );
        assert_security_headers(&headers);
    }
}
