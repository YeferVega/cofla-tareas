use axum::http::header::{
    HeaderValue, CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::Router;
use tower::service_fn;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::routes::{health, not_found, tasks};
use crate::state::AppState;

const CSP: &str = "default-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'self'";

pub fn build_router(state: AppState) -> Router {
    let static_service = ServeDir::new(state.static_dir.clone())
        .call_fallback_on_method_not_allowed(true)
        .not_found_service(service_fn(serve_not_found));

    Router::new()
        .route("/health", get(health::health).fallback(not_found))
        .route(
            "/api/tasks",
            get(tasks::list).post(tasks::create).fallback(not_found),
        )
        .route(
            "/api/tasks/{id}",
            patch(tasks::patch)
                .delete(tasks::delete)
                .fallback(not_found),
        )
        .with_state(state)
        .fallback_service(static_service)
        .layer(SetResponseHeaderLayer::overriding(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
}

async fn serve_not_found(
    _request: axum::extract::Request,
) -> Result<axum::http::Response<axum::body::Body>, std::convert::Infallible> {
    Ok(axum::http::Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(CONTENT_TYPE, "application/json; charset=utf-8")
        .header(CACHE_CONTROL, "no-store")
        .body(axum::body::Body::from("{\"error\":\"Not found\"}"))
        .expect("static not-found response should be buildable"))
}
