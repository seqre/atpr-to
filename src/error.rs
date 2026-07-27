use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Build a plain-text error response.
///
// TODO(rebrand): restore a styled error page. `templates/error.html` and its
// `ErrorTemplate { status: u16, title: &str, message: &str }` were deleted for
// the rebrand, so every error in the app is currently bare text.
//
// When reinstating: keep this plain-text branch as the render-failure fallback,
// otherwise a template error recurses through `internal_error` forever.
//
// `message` is attacker-influenced on some paths (it interpolates handles,
// codes, and upstream error strings). Askama's auto-escaping was the only XSS
// guard — the deleted `test_xss_escaping` covered exactly that. Any replacement
// template must keep `message` escaped, and the test must come back with it.
pub fn error_page(status: StatusCode, title: &str, message: &str) -> Response {
    (
        status,
        format!("{} {}\n{}", status.as_u16(), title, message),
    )
        .into_response()
}

/// Return a 404 Not Found error response.
pub fn not_found(message: &str) -> Response {
    error_page(StatusCode::NOT_FOUND, "Not Found", message)
}

/// Return a 400 Bad Request error response.
pub fn bad_request(message: &str) -> Response {
    error_page(StatusCode::BAD_REQUEST, "Bad Request", message)
}

/// Return a 401 Unauthorized error response.
pub fn unauthorized(message: &str) -> Response {
    error_page(StatusCode::UNAUTHORIZED, "Unauthorized", message)
}

/// Return a 502 Bad Gateway error response.
pub fn bad_gateway(message: &str) -> Response {
    error_page(StatusCode::BAD_GATEWAY, "Bad Gateway", message)
}

/// Return a 410 Gone error response.
pub fn gone(message: &str) -> Response {
    error_page(StatusCode::GONE, "Gone", message)
}

/// Return a 500 Internal Server Error error response.
pub fn internal_error(message: &str) -> Response {
    error_page(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Internal Server Error",
        message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    async fn body_string(body: Body) -> String {
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn test_error_page_status() {
        let resp = bad_gateway("upstream down");
        let (parts, body) = resp.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_GATEWAY);
        let body = body_string(body).await;
        assert!(body.contains("502"));
    }

    #[tokio::test]
    async fn test_not_found_status() {
        let resp = not_found("page missing");
        let (parts, _) = resp.into_parts();
        assert_eq!(parts.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_bad_request_status() {
        let resp = bad_request("invalid input");
        let (parts, _) = resp.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_unauthorized_status() {
        let resp = unauthorized("not logged in");
        let (parts, _) = resp.into_parts();
        assert_eq!(parts.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_internal_error_status() {
        let resp = internal_error("something went wrong");
        let (parts, _) = resp.into_parts();
        assert_eq!(parts.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_gone_status() {
        let resp = gone("link expired");
        let (parts, body) = resp.into_parts();
        assert_eq!(parts.status, StatusCode::GONE);
        let body = body_string(body).await;
        assert!(body.contains("410"));
        assert!(body.contains("link expired"));
    }
}
