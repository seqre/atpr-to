//! Static file serving with `rust-embed` and configurable `Cache-Control`.
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

use crate::AppState;

/// Embedded static assets from the `static/` directory.
#[derive(Embed)]
#[folder = "static/"]
pub struct StaticAssets;

/// Serve an embedded static file with the correct MIME type and Cache-Control header.
pub async fn static_file(State(state): State<Arc<AppState>>, Path(path): Path<String>) -> Response {
    match StaticAssets::get(&path) {
        Some(file) => {
            let mime = mime_guess::from_path(&path)
                .first_or_octet_stream()
                .to_string();
            let max_age = state.config.static_cache_max_age;
            (
                [
                    (header::CONTENT_TYPE, mime),
                    (header::CACHE_CONTROL, format!("public, max-age={max_age}")),
                ],
                file.data,
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::router;

    #[tokio::test]
    async fn test_static_app_css_ok() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/static/app.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("text/css"), "expected text/css, got {ct}");
    }

    #[tokio::test]
    async fn test_static_nonexistent_returns_404() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/static/nonexistent.xyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_static_cache_control_header() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/static/app.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let cc = response
            .headers()
            .get("cache-control")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            cc.contains("max-age="),
            "expected max-age in Cache-Control, got {cc}"
        );
    }
}
