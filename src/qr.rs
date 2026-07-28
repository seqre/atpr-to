use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use qrcode::render::svg;
use qrcode::QrCode;

use crate::error::AppError;
use crate::AppState;

/// Generate a QR code for a short URL, returned as SVG.
pub async fn qr_code(
    State(state): State<Arc<AppState>>,
    Path((handle, code)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let url = state.config.base_url.short_url(&handle, &code);
    let qr = QrCode::new(url.as_bytes()).map_err(AppError::internal)?;
    let svg = qr.render::<svg::Color>().min_dimensions(200, 200).build();

    Ok((
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("image/svg+xml"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=86400"),
            ),
        ],
        svg,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::router;

    #[tokio::test]
    async fn test_qr_route_returns_svg() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/@alice.bsky.social/abc123/qr")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response.headers().get("content-type").unwrap();
        assert_eq!(content_type, "image/svg+xml");
    }

    #[tokio::test]
    async fn test_qr_contains_svg_tag() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/@alice.bsky.social/abc123/qr")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("<svg"));
    }
}
