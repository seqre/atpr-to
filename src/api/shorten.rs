//! `POST /api/shorten` — create a short link.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthedUser, Authenticator};
use crate::domain::{ShortCode, TargetUrl};
use crate::error::AppError;
use crate::store::{LinkStore, PutMode};
use crate::AppState;

/// Request body for `POST /api/shorten`.
#[derive(Deserialize)]
pub struct ShortenRequest {
    /// The destination URL to shorten.
    pub url: String,
    /// Optional custom short code; auto-generated if absent.
    pub code: Option<String>,
}

/// Response body from `POST /api/shorten`.
#[derive(Serialize)]
pub struct ShortenResponse {
    /// The resulting short URL (e.g. `https://atpr.to/@alice/abc123`).
    pub short_url: String,
}

/// Create a short URL. Requires authentication.
///
/// This was 100 lines and six match ladders, most of them re-deriving values the
/// type system now carries: the DID was re-parsed, the URL was parsed three
/// times, and the repo/collection/rkey preamble was written out inline.
#[tracing::instrument(skip_all, fields(code))]
pub async fn shorten<A: Authenticator>(
    State(state): State<Arc<AppState<A>>>,
    user: AuthedUser<A>,
    Json(body): Json<ShortenRequest>,
) -> Result<Json<ShortenResponse>, AppError> {
    let code = match &body.code {
        Some(c) => ShortCode::parse(c)?,
        None => ShortCode::generate(),
    };
    let target = TargetUrl::parse(&body.url)?;
    tracing::Span::current().record("code", code.as_str());

    // CreateOnly, so re-using a code returns 409 rather than silently
    // destroying the link that was already there.
    user.store
        .put(&code, &target, PutMode::CreateOnly)
        .await
        .map_err(AppError::from)?;

    // The short URL's identity segment must be a *handle*: `resolve` parses it
    // with `Handle::new_owned`, and a DID is not a valid handle. Falling back to
    // the DID string minted a URL that could never resolve, while reporting
    // success.
    let did_str = user.did.as_ref().to_string();
    let Some(handle) = state.identity.handle_for(&did_str).await else {
        tracing::error!(did = %did_str, "record written but handle resolution failed");
        return Err(AppError::Upstream(anyhow::anyhow!(
            "handle resolution failed for {did_str} after the record was written"
        )));
    };

    Ok(Json(ShortenResponse {
        short_url: state.config.base_url.short_url(&handle, code.as_str()),
    }))
}
