use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ShortenRequest {
    pub url: String,
    pub code: Option<String>,
}

#[derive(Serialize)]
pub struct ShortenResponse {
    pub short_url: String,
}

/// Create a short URL. Requires authentication.
pub async fn shorten(
    Json(_body): Json<ShortenRequest>,
) -> (StatusCode, &'static str) {
    // TODO: validate auth, generate code, write record to PDS
    (StatusCode::UNAUTHORIZED, "Authentication required")
}
