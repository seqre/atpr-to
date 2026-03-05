use askama::Template;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorTemplate<'a> {
    status: u16,
    title: &'a str,
    message: &'a str,
}

/// Build a styled HTML error page.
pub fn error_page(status: StatusCode, title: &str, message: &str) -> Response {
    let tmpl = ErrorTemplate {
        status: status.as_u16(),
        title,
        message,
    };
    match tmpl.render() {
        Ok(html) => (status, Html(html)).into_response(),
        // Fallback to plain text to avoid infinite recursion if template rendering fails.
        Err(_) => (status, title.to_string()).into_response(),
    }
}

/// Return a 404 Not Found HTML error response.
pub fn not_found(message: &str) -> Response {
    error_page(StatusCode::NOT_FOUND, "Not Found", message)
}

/// Return a 400 Bad Request HTML error response.
pub fn bad_request(message: &str) -> Response {
    error_page(StatusCode::BAD_REQUEST, "Bad Request", message)
}

/// Return a 401 Unauthorized HTML error response.
pub fn unauthorized(message: &str) -> Response {
    error_page(StatusCode::UNAUTHORIZED, "Unauthorized", message)
}

/// Return a 502 Bad Gateway HTML error response.
pub fn bad_gateway(message: &str) -> Response {
    error_page(StatusCode::BAD_GATEWAY, "Bad Gateway", message)
}

/// Return a 410 Gone HTML error response.
pub fn gone(message: &str) -> Response {
    error_page(StatusCode::GONE, "Gone", message)
}

/// Return a 500 Internal Server Error HTML error response.
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
    async fn test_error_page_html() {
        let resp = error_page(StatusCode::NOT_FOUND, "Not Found", "test message");
        let (parts, body) = resp.into_parts();
        let html = body_string(body).await;
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Not Found"));
        assert!(html.contains("test message"));
        assert_eq!(parts.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_error_page_status() {
        let resp = bad_gateway("upstream down");
        let (parts, body) = resp.into_parts();
        assert_eq!(parts.status, StatusCode::BAD_GATEWAY);
        let html = body_string(body).await;
        assert!(html.contains("502"));
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
        let html = body_string(body).await;
        assert!(html.contains("410"));
        assert!(html.contains("link expired"));
    }

    #[tokio::test]
    async fn test_xss_escaping() {
        let resp = error_page(
            StatusCode::BAD_REQUEST,
            "Bad Request",
            "<script>alert(1)</script>",
        );
        let (_, body) = resp.into_parts();
        let html = body_string(body).await;
        assert!(!html.contains("<script>alert(1)</script>"));
        // Askama escapes < as &#60; and > as &#62;
        assert!(html.contains("&#60;script&#62;"));
    }
}
