//! `GET /@{handle}/{code}/qr` — the short URL as an SVG QR code.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};

use crate::api::shortlink::qr_svg;
use crate::auth::Authenticator;
use crate::error::AppError;
use crate::AppState;

/// Generate a QR code for a short URL, returned as SVG.
///
/// Does not resolve the link: this encodes the short URL itself, so it is
/// correct even for a code that does not exist yet.
pub async fn qr_code<A: Authenticator>(
    State(state): State<Arc<AppState<A>>>,
    Path((handle, code)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let url = state.config.base_url.short_url(&handle, &code);

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
        qr_svg(&url)?,
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
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "image/svg+xml"
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains("<svg"));
    }
}
