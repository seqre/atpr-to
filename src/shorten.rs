use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::AppState;

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
    State(_state): State<Arc<AppState>>,
    Json(_body): Json<ShortenRequest>,
) -> (StatusCode, &'static str) {
    // TODO: validate auth, generate code, write record to PDS
    (StatusCode::UNAUTHORIZED, "Authentication required")
}
