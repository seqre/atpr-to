use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

/// Build a styled HTML error page.
pub fn error_page(status: StatusCode, title: &str, message: &str) -> Response {
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{status} — {title}</title>
<style>
  body {{ font-family: system-ui, sans-serif; max-width: 40em; margin: 4em auto; padding: 0 1em; color: #333; }}
  h1 {{ font-size: 2em; margin-bottom: 0.25em; }}
  .status {{ color: #888; font-size: 0.9em; }}
  a {{ color: #0066cc; }}
</style>
</head>
<body>
<h1>{title}</h1>
<p class="status">{status}</p>
<p>{message}</p>
<p><a href="/">← atpr.to</a></p>
</body>
</html>"#,
        status = status.as_u16(),
        title = title,
        message = message,
    );
    (status, Html(html)).into_response()
}

pub fn not_found(message: &str) -> Response {
    error_page(StatusCode::NOT_FOUND, "Not Found", message)
}

pub fn bad_request(message: &str) -> Response {
    error_page(StatusCode::BAD_REQUEST, "Bad Request", message)
}

pub fn unauthorized(message: &str) -> Response {
    error_page(StatusCode::UNAUTHORIZED, "Unauthorized", message)
}

pub fn bad_gateway(message: &str) -> Response {
    error_page(StatusCode::BAD_GATEWAY, "Bad Gateway", message)
}

pub fn internal_error(message: &str) -> Response {
    error_page(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error", message)
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
}
