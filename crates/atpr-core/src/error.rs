//! The application error type.
//!
//! This is the only place in the crate that decides an HTTP status code, and the
//! only place that decides what an error body says. Before this existed there
//! were three competing conventions — `error::*` helper functions, raw
//! `(StatusCode, String)` tuples, and bare `StatusCode` — every handler was a
//! match ladder, and eight call sites interpolated upstream error text straight
//! into responses served to anonymous clients.
//!
//! Bodies are JSON everywhere, including on the browser-facing routes. The old
//! `templates/error.html` was deleted in the rebrand, so those routes were
//! already emitting bare text; JSON is no worse for a human and removes the
//! trap of `/api/*` endpoints answering `text/plain`. When a styled error page
//! comes back, `IntoResponse` is the single place to add negotiation.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

/// The body message for [`AppError::HandleNotFound`].
///
/// Shared so the HTML error page can recognise its own 404 without matching on
/// a string literal spelled out twice.
pub const HANDLE_NOT_FOUND: &str = "handle not found";

/// Everything a handler can fail with.
///
/// `BadRequest` takes `&'static str` deliberately: it makes interpolating an
/// upstream error message into a client-visible body a *compile* error rather
/// than something a reviewer has to notice. Where detail matters for debugging,
/// attach it to `Upstream`/`Internal`, which are logged in full and rendered
/// opaquely.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The requested resource does not exist.
    #[error("not found")]
    NotFound,

    /// The handle in the address does not resolve to an account.
    ///
    /// A 404 like [`Self::NotFound`], but a different sentence: "this person
    /// has no account here" and "this person deleted that link" are different
    /// facts, and a visitor who mistyped a handle can act on the first one.
    /// The message is a constant rather than a literal because
    /// `api::error_page` matches on it to choose the page's heading — keying
    /// off our own value, never off prose that could drift.
    #[error("{HANDLE_NOT_FOUND}")]
    HandleNotFound,

    /// The request was malformed. The message is server-authored and safe to
    /// return verbatim.
    #[error("{0}")]
    BadRequest(&'static str),

    /// No valid session.
    #[error("unauthorized")]
    Unauthorized,

    /// The short code is already taken.
    #[error("short code already in use")]
    Conflict,

    /// An upstream call (PDS, Slingshot, identity resolution) failed.
    #[error("upstream unavailable")]
    Upstream(#[source] anyhow::Error),

    /// Anything else — a bug on our side.
    #[error("internal error")]
    Internal(#[source] anyhow::Error),
}

impl AppError {
    /// The status code this error maps to.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound | Self::HandleNotFound => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Wrap any error as an internal failure.
    pub fn internal(e: impl Into<anyhow::Error>) -> Self {
        Self::Internal(e.into())
    }

    /// Wrap any error as an upstream failure.
    pub fn upstream(e: impl Into<anyhow::Error>) -> Self {
        Self::Upstream(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Log the full source chain before discarding it. The `error::*` helpers
        // this replaces emitted no tracing event at all, so 500s were invisible
        // in CloudWatch.
        match &self {
            Self::Upstream(e) | Self::Internal(e) => {
                tracing::error!(chain = %format_chain(e), "request failed");
            }
            _ => {}
        }

        let status = self.status();
        // Opaque for anything carrying an upstream message. Only `BadRequest`'s
        // server-authored text reaches the client.
        let message = self.to_string();

        (
            status,
            // Successful redirects are cached; failures must not be. A cached
            // 404 outlives the link that fixes it — someone shortens a code,
            // and everyone who probed it first keeps getting the 404 for the
            // whole TTL.
            [(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))],
            axum::Json(serde_json::json!({ "error": message })),
        )
            .into_response()
    }
}

/// Render an error and its source chain as `a: b: c`.
fn format_chain(e: &anyhow::Error) -> String {
    e.chain()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(": ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    async fn body_json(body: Body) -> serde_json::Value {
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn test_status_mapping() {
        assert_eq!(AppError::NotFound.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            AppError::BadRequest("bad").status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(AppError::Unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(AppError::Conflict.status(), StatusCode::CONFLICT);
        assert_eq!(
            AppError::upstream(anyhow::anyhow!("x")).status(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            AppError::internal(anyhow::anyhow!("x")).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn test_bad_request_message_is_returned() {
        let resp = AppError::BadRequest("code must be 1-64 chars").into_response();
        let (parts, body) = resp.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
        assert_eq!(body_json(body).await["error"], "code must be 1-64 chars");
    }

    /// The whole point of the type: upstream text must not reach the client.
    #[tokio::test]
    async fn test_upstream_body_is_opaque() {
        let secret = "postgres://user:hunter2@10.0.0.1/internal";
        let resp = AppError::upstream(anyhow::anyhow!("connect failed: {secret}")).into_response();
        let (parts, body) = resp.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_GATEWAY);

        let json = body_json(body).await;
        assert_eq!(json["error"], "upstream unavailable");
        assert!(
            !json.to_string().contains("hunter2"),
            "upstream detail leaked into the response body"
        );
    }

    #[tokio::test]
    async fn test_internal_body_is_opaque() {
        let resp =
            AppError::internal(anyhow::anyhow!("index out of bounds at foo.rs:42")).into_response();
        let (parts, body) = resp.into_parts();
        assert_eq!(parts.status, StatusCode::INTERNAL_SERVER_ERROR);
        let json = body_json(body).await;
        assert_eq!(json["error"], "internal error");
        assert!(!json.to_string().contains("foo.rs"));
    }

    #[tokio::test]
    async fn test_error_body_is_json() {
        let resp = AppError::NotFound.into_response();
        let (parts, body) = resp.into_parts();
        assert_eq!(
            parts.headers.get("content-type").unwrap(),
            "application/json"
        );
        assert_eq!(body_json(body).await["error"], "not found");
    }

    #[test]
    fn test_format_chain_includes_sources() {
        let inner = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let e = anyhow::Error::from(inner).context("reading sessions");
        let chain = format_chain(&e);
        assert!(chain.contains("reading sessions"), "{chain}");
        assert!(chain.contains("no such file"), "{chain}");
    }
}
