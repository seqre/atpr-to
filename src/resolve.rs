use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Resolve a short URL and redirect.
/// Route: GET /@{handle}/{code}
pub async fn resolve(Path((handle, code)): Path<(String, String)>) -> Response {
    // TODO: resolve handle → DID → PDS, fetch record, redirect
    let _ = (&handle, &code);
    (StatusCode::BAD_GATEWAY, "Resolution not yet implemented").into_response()
}
