use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use axum::body::to_bytes;
use axum::http::header::{CONTENT_SECURITY_POLICY, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

use cofla_tareas_backend::app::build_router;
use cofla_tareas_backend::state::AppState;

const CSP: &str = "default-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'self'";
const INDEX_BODY: &str = "<!doctype html><html><head><title>Cofla Tareas</title></head><body><div id=\"root\">app-root-marker</div></body></html>";

fn temp_dist_dir(label: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after the epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "cofla-static-{label}-{}-{unique}",
        std::process::id()
    ));
    let assets = dir.join("assets");
    fs::create_dir_all(&assets).expect("temp static dir should be creatable");
    fs::write(dir.join("index.html"), INDEX_BODY).expect("index.html should be writable");
    fs::write(assets.join("app.js"), "console.log('cofla');\n").expect("app.js should be writable");
    fs::write(assets.join("app.css"), "body { margin: 0; }\n").expect("app.css should be writable");
    dir
}

fn app_with_dist(label: &str) -> (Router, PathBuf) {
    let dir = temp_dist_dir(label);
    let app = build_router(AppState::memory("1.0").with_static_dir(dir.clone()));
    (app, dir)
}

async fn get(app: Router, uri: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    (status, headers, bytes.to_vec())
}

fn header_text(headers: &HeaderMap, name: axum::http::HeaderName) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

fn assert_security_headers(headers: &HeaderMap) {
    assert_eq!(
        header_text(headers, CONTENT_SECURITY_POLICY),
        CSP,
        "baseline CSP header missing"
    );
    assert_eq!(
        header_text(headers, X_CONTENT_TYPE_OPTIONS),
        "nosniff",
        "nosniff header missing"
    );
}

fn assert_json_404(status: StatusCode, headers: &HeaderMap, body: &[u8]) {
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "body: {}",
        String::from_utf8_lossy(body)
    );
    let parsed: Value = serde_json::from_slice(body).unwrap_or_else(|err| {
        panic!(
            "body is not JSON ({err}): {}",
            String::from_utf8_lossy(body)
        )
    });
    assert_eq!(parsed, serde_json::json!({ "error": "Not found" }));
    assert!(header_text(headers, CONTENT_TYPE).starts_with("application/json"));
    assert_security_headers(headers);
}

#[tokio::test]
async fn root_serves_index_html_with_security_headers() {
    let (app, dir) = app_with_dist("root");
    let (status, headers, body) = get(app, "/").await;
    fs::remove_dir_all(&dir).expect("temp dir cleanup should succeed");

    assert_eq!(status, StatusCode::OK);
    assert!(
        header_text(&headers, CONTENT_TYPE).starts_with("text/html"),
        "content-type: {}",
        header_text(&headers, CONTENT_TYPE)
    );
    assert_security_headers(&headers);
    assert!(
        String::from_utf8_lossy(&body).contains("app-root-marker"),
        "index.html body not served"
    );
}

#[tokio::test]
async fn compiled_assets_are_served_with_expected_mime_types() {
    let (app, _dir) = app_with_dist("assets");

    let (status, headers, body) = get(app.clone(), "/assets/app.js").await;
    assert_eq!(status, StatusCode::OK);
    let mime = header_text(&headers, CONTENT_TYPE);
    assert!(
        mime.starts_with("text/javascript") || mime.starts_with("application/javascript"),
        "js mime: {mime}"
    );
    assert_security_headers(&headers);
    assert!(String::from_utf8_lossy(&body).contains("cofla"));

    let (status, headers, _body) = get(app, "/assets/app.css").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        header_text(&headers, CONTENT_TYPE).starts_with("text/css"),
        "css mime: {}",
        header_text(&headers, CONTENT_TYPE)
    );
    assert_security_headers(&headers);
}

#[tokio::test]
async fn path_traversal_attempts_return_json_404() {
    let (app, _dir) = app_with_dist("traversal");

    for uri in [
        "/../Cargo.toml",
        "/assets/../../Cargo.toml",
        "/assets/../../etc/passwd",
        "/%2e%2e/Cargo.toml",
        "/%2E%2E/%2E%2E/Cargo.toml",
        "/..%2fCargo.toml",
    ] {
        let (status, headers, body) = get(app.clone(), uri).await;
        assert_json_404(status, &headers, &body);
    }
}

#[tokio::test]
async fn unknown_paths_return_json_404_and_never_the_spa_shell() {
    let (app, _dir) = app_with_dist("unknown");

    for uri in [
        "/nonexistent",
        "/assets/missing.js",
        "/tasks/12",
        "/api",
        "/api/unknown",
        "/api/tasks/1/extra",
        "/healthz",
    ] {
        let (status, headers, body) = get(app.clone(), uri).await;
        assert_json_404(status, &headers, &body);
    }
}

#[tokio::test]
async fn wrong_methods_on_static_paths_return_json_404_never_405() {
    let (app, _dir) = app_with_dist("methods");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), 65536).await.unwrap();
    assert_json_404(status, &headers, &body);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/assets/app.js")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), 65536).await.unwrap();
    assert_json_404(status, &headers, &body);
}

#[tokio::test]
async fn missing_static_dir_keeps_api_serving_and_paths_on_json_404() {
    let app =
        build_router(AppState::memory("1.0").with_static_dir("/nonexistent-directory-for-test"));
    let (status, _headers, body) = get(app, "/").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let parsed: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed, serde_json::json!({ "error": "Not found" }));
}

struct ServerProcess {
    child: Child,
    stdout: Option<BufReader<std::process::ChildStdout>>,
}

impl ServerProcess {
    fn new(mut child: Child) -> Self {
        let stdout = child.stdout.take().map(BufReader::new);
        Self { child, stdout }
    }

    fn startup_port(&mut self) -> u16 {
        let reader = self.stdout.as_mut().expect("stdout should be piped");
        loop {
            let mut line = String::new();
            let read = reader.read_line(&mut line).unwrap_or(0);
            if read == 0 {
                panic!("server exited before printing the startup line");
            }
            if line.starts_with("Cofla Tareas") {
                let mut tokens = line.split_whitespace();
                while let Some(token) = tokens.next() {
                    if token == "port" {
                        if let Some(port) = tokens.next().and_then(|value| value.parse().ok()) {
                            return port;
                        }
                    }
                }
                panic!("startup line has no usable port token: {line}");
            }
        }
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_static_server(static_dir: &std::path::Path) -> ServerProcess {
    ServerProcess::new(
        Command::new(env!("CARGO_BIN_EXE_cofla-tareas-backend"))
            .env("PORT", "0")
            .env("STORAGE", "memory")
            .env("APP_VERSION", "static-e2e")
            .env("STATIC_DIR", static_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("server binary should start"),
    )
}

#[test]
fn static_dir_env_var_is_honoured_by_the_real_binary() {
    let dir = temp_dist_dir("env");
    let mut server = spawn_static_server(&dir);
    let port = server.startup_port();

    let mut stream =
        TcpStream::connect(("127.0.0.1", port)).expect("server should accept connections");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout settable");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .expect("request writable");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("response readable");

    drop(server);
    fs::remove_dir_all(&dir).expect("temp dir cleanup");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "got: {response}");
    assert!(response.contains("text/html"), "got: {response}");
    assert!(response.contains("app-root-marker"), "got: {response}");
    assert!(
        response
            .to_ascii_lowercase()
            .contains("content-security-policy"),
        "got: {response}"
    );
    assert!(response
        .to_ascii_lowercase()
        .contains("x-content-type-options: nosniff"));
}
