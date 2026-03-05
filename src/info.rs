use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::{Html, IntoResponse, Response};
use qrcode::render::svg;
use qrcode::QrCode;

use crate::error;
use crate::resolve::resolve_via_slingshot;
use crate::AppState;

/// Escape HTML special characters to prevent XSS.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Show a preview page for a short link: destination URL, creation date, optional expiry, and QR code.
#[tracing::instrument(skip(state), fields(handle, code))]
pub async fn info(
    State(state): State<Arc<AppState>>,
    Path((handle, code)): Path<(String, String)>,
) -> Response {
    let link =
        match resolve_via_slingshot(&state.http, &state.config.slingshot_url, &handle, &code).await
        {
            Ok(l) => l,
            Err(e) => {
                if e.to_string().contains("404") {
                    return error::not_found("Link not found");
                }
                return error::bad_gateway(&format!("Could not resolve link: {e}"));
            }
        };

    let short_url = format!("{}/@{}/{}", state.config.base_url, handle, code);
    let qr_svg = QrCode::new(short_url.as_bytes())
        .map(|qr| qr.render::<svg::Color>().min_dimensions(200, 200).build())
        .unwrap_or_default();

    let created_row = link
        .created_at
        .as_deref()
        .map(|c| format!("<p><strong>Created:</strong> {}</p>", html_escape(c)))
        .unwrap_or_default();

    let expires_row = link
        .expires_at
        .as_deref()
        .map(|e| format!("<p><strong>Expires:</strong> {}</p>", html_escape(e)))
        .unwrap_or_default();

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Link preview — atpr.to</title>
<style>
  body {{ font-family: system-ui, sans-serif; max-width: 40em; margin: 4em auto; padding: 0 1em; color: #333; }}
  a {{ color: #0066cc; word-break: break-all; }}
</style>
</head>
<body>
<h1>Link Preview</h1>
<p><strong>Destination:</strong> <a href="{url}">{url_escaped}</a></p>
{created}{expires}
<p><a href="/@{handle_e}/{code_e}">Follow this link →</a></p>
{qr}
<p><a href="/">← atpr.to</a></p>
</body>
</html>"#,
        url = html_escape(&link.url),
        url_escaped = html_escape(&link.url),
        created = created_row,
        expires = expires_row,
        handle_e = html_escape(&handle),
        code_e = html_escape(&code),
        qr = qr_svg,
    );

    Html(html).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("hello"), "hello");
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape(r#"say "hi""#), "say &quot;hi&quot;");
        assert_eq!(html_escape("safe-code_123"), "safe-code_123");
    }
}
