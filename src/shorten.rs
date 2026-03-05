use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::CookieJar;
use jacquard::api::com_atproto::repo::put_record::PutRecord;
use jacquard_common::types::collection::Collection;
use jacquard_common::types::did::Did;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::value::to_data;
use jacquard_common::xrpc::XrpcClient;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::auth::parse_session_cookie;
use crate::generated::to_atpr::link::Link;
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

const CODE_CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Generate a random short code (6-8 alphanumeric chars).
fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    let len = rng.gen_range(6..=8);
    (0..len)
        .map(|_| CODE_CHARSET[rng.gen_range(0..CODE_CHARSET.len())] as char)
        .collect()
}

/// Validate a short code: alphanumeric + `-_`, 1-64 chars.
fn validate_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 64
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Resolve a DID to its primary handle.
///
/// Tries Slingshot's `describeRepo` first (1 hop), falls back to direct DID doc resolution.
/// Returns `None` if both fail (caller should use DID string instead).
async fn resolve_did_to_handle(
    client: &reqwest::Client,
    slingshot_url: &str,
    did_str: &str,
) -> Option<String> {
    // Try Slingshot describeRepo first
    let url = format!(
        "{}/xrpc/com.atproto.repo.describeRepo?repo={}",
        slingshot_url.trim_end_matches('/'),
        urlencoding::encode(did_str),
    );
    if let Ok(resp) = client.get(&url).send().await {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(handle) = body.get("handle").and_then(|h| h.as_str()) {
                    return Some(handle.to_string());
                }
            }
        }
    }

    // Fallback: resolve DID doc directly
    use jacquard::identity::resolver::IdentityResolver;
    use jacquard::identity::JacquardResolver;
    let did = Did::new(did_str).ok()?;
    let resolver = JacquardResolver::default();
    let doc_response = resolver.resolve_did_doc(&did).await.ok()?;
    let doc = doc_response.parse().ok()?;
    doc.handles().into_iter().next().map(|h| h.as_ref().to_string())
}

/// Create a short URL. Requires authentication.
#[tracing::instrument(skip_all)]
pub async fn shorten(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(body): Json<ShortenRequest>,
) -> Response {
    // Check auth
    let Some((did_str, session_id)) = parse_session_cookie(&jar) else {
        return (StatusCode::UNAUTHORIZED, "Authentication required").into_response();
    };

    // Determine short code
    let code = match &body.code {
        Some(c) => {
            if !validate_code(c) {
                return (
                    StatusCode::BAD_REQUEST,
                    "Invalid code: must be 1-64 chars, alphanumeric or -_",
                )
                    .into_response();
            }
            c.clone()
        }
        None => generate_code(),
    };

    // Validate URL
    if body.url.len() > 2048 {
        return (StatusCode::BAD_REQUEST, "URL too long (max 2048 chars)").into_response();
    }
    if url::Url::parse(&body.url).is_err() {
        return (StatusCode::BAD_REQUEST, "Invalid URL").into_response();
    }

    // Restore OAuth session
    let did = match Did::new(&did_str) {
        Ok(d) => d,
        Err(_) => {
            return (StatusCode::UNAUTHORIZED, "Invalid session").into_response();
        }
    };

    let session = match state.oauth.restore(&did, &session_id).await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::UNAUTHORIZED,
                format!("Session expired: {e}"),
            )
                .into_response();
        }
    };

    // Build the link record
    let link_url = match jacquard_common::types::string::Uri::new(&body.url) {
        Ok(u) => u,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("Invalid URI: {e}"),
            )
                .into_response();
        }
    };

    let record = Link::new()
        .url(link_url)
        .created_at(chrono::Utc::now().fixed_offset())
        .build();

    // Serialize to Data for the XRPC request
    let data = match to_data(&record) {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to serialize record: {e}"),
            )
                .into_response();
        }
    };

    // Build putRecord request
    let rkey = match jacquard_common::types::string::RecordKey::any(&code) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("Invalid record key: {e}"),
            )
                .into_response();
        }
    };

    let request = PutRecord::new()
        .repo(AtIdentifier::Did(did))
        .collection(<Link as Collection>::NSID.to_string())
        .rkey(rkey)
        .record(data)
        .build();

    // Send the request
    match session.send(request).await {
        Ok(_response) => {
            // Best-effort: resolve DID → handle for a nicer short URL.
            // Falls back to DID string if resolution fails.
            let display_ident = resolve_did_to_handle(&state.http, &state.slingshot_url, &did_str)
                .await
                .unwrap_or(did_str);
            let short_url = format!("https://atpr.to/@{}/{}", display_ident, code);
            Json(ShortenResponse { short_url }).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create record: {e}"),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_length() {
        for _ in 0..100 {
            let code = generate_code();
            assert!((6..=8).contains(&code.len()));
            assert!(validate_code(&code));
        }
    }

    #[test]
    fn test_validate_code_valid() {
        assert!(validate_code("abc123"));
        assert!(validate_code("my-code"));
        assert!(validate_code("my_code"));
        assert!(validate_code("A"));
        assert!(validate_code(&"a".repeat(64)));
    }

    #[test]
    fn test_validate_code_invalid() {
        assert!(!validate_code(""));
        assert!(!validate_code(&"a".repeat(65)));
        assert!(!validate_code("has spaces"));
        assert!(!validate_code("has/slash"));
        assert!(!validate_code("has.dot"));
    }
}
