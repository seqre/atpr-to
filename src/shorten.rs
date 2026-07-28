use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use jacquard::api::com_atproto::repo::put_record::PutRecord;
use jacquard_common::types::collection::Collection;
use jacquard_common::types::did::Did;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::types::recordkey::{RecordKey, Rkey};
use jacquard_common::types::string::Datetime;
use jacquard_common::types::uri::UriValue;
use jacquard_common::types::value::to_data;
use jacquard_common::xrpc::XrpcClient;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthSession, Resolver};
use crate::error::AppError;
use crate::generated::to_atpr::link::Link;
use crate::AppState;

/// Request body for `POST /shorten`.
#[derive(Deserialize)]
pub struct ShortenRequest {
    /// The destination URL to shorten.
    pub url: String,
    /// Optional custom short code; auto-generated if absent.
    pub code: Option<String>,
}

/// Response body from `POST /shorten`.
#[derive(Serialize)]
pub struct ShortenResponse {
    /// The resulting short URL (e.g. `https://atpr.to/@alice/abc123`).
    pub short_url: String,
}

const CODE_CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Maximum destination URL length, mirroring `maxLength` on `url` in
/// `lexicons/to/atpr/link.json`. Phase 3 pins the two together with a test.
const MAX_TARGET_LEN: usize = 2048;

/// Generate a random short code (6-8 alphanumeric chars).
fn generate_code() -> String {
    let mut rng = rand::rng();
    let len = 6;
    (0..len)
        .map(|_| CODE_CHARSET[rng.random_range(0..CODE_CHARSET.len())] as char)
        .collect()
}

/// Validate a short code: alphanumeric + `-_`, 1-64 chars.
pub fn validate_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 64
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Returns `true` if the URL has an allowed scheme (`http` or `https`).
pub fn is_allowed_scheme(url_str: &str) -> bool {
    url::Url::parse(url_str)
        .map(|u| matches!(u.scheme(), "http" | "https"))
        .unwrap_or(false)
}

/// Resolve a DID to its primary handle.
///
/// Tries Slingshot's `describeRepo` first (1 hop), falls back to direct DID doc resolution.
/// Returns `None` if both fail (caller should use DID string instead).
// coverage:excl-start
pub(crate) async fn resolve_did_to_handle(
    client: &reqwest::Client,
    resolver: &Resolver,
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
    let did: Did = Did::new_owned(did_str).ok()?;
    let doc_response = resolver.resolve_did_doc(&did).await.ok()?;
    let doc = doc_response.parse().ok()?;
    doc.handles()
        .into_iter()
        .next()
        .map(|h| h.as_ref().to_string())
}
// coverage:excl-stop

/// Create a short URL. Requires authentication.
#[tracing::instrument(skip_all)]
// coverage:excl-start
pub async fn shorten(
    State(state): State<Arc<AppState>>,
    auth: AuthSession,
    Json(body): Json<ShortenRequest>,
) -> Result<Json<ShortenResponse>, AppError> {
    let AuthSession(session) = auth;
    let (did, _) = session.session_info().await;
    let did_str = did.as_ref().to_string();

    let code = match &body.code {
        Some(c) if !validate_code(c) => {
            return Err(AppError::BadRequest(
                "Invalid code: must be 1-64 chars, alphanumeric or -_",
            ))
        }
        Some(c) => c.clone(),
        None => generate_code(),
    };

    if body.url.len() > MAX_TARGET_LEN {
        return Err(AppError::BadRequest("URL too long (max 2048 chars)"));
    }
    if url::Url::parse(&body.url).is_err() {
        return Err(AppError::BadRequest("Invalid URL"));
    }
    if !is_allowed_scheme(&body.url) {
        return Err(AppError::BadRequest("Only http/https URLs are allowed"));
    }

    let link_url: UriValue =
        UriValue::new_owned(&body.url).map_err(|_| AppError::BadRequest("Invalid URL"))?;

    let record: Link = Link::new()
        .url(link_url)
        .updated_at(Datetime::now())
        .build();

    let data = to_data(&record).map_err(AppError::internal)?;
    let rkey: RecordKey<Rkey> =
        RecordKey::any_owned(&code).map_err(|_| AppError::BadRequest("Invalid code"))?;
    let owned_did: Did = Did::new_owned(&did_str).map_err(|_| AppError::Unauthorized)?;
    let collection = Nsid::new_static(<Link as Collection>::NSID).expect("valid NSID");

    let request = PutRecord::new()
        .repo(AtIdentifier::Did(owned_did))
        .collection(collection)
        .rkey(rkey)
        .record(data)
        .build();

    session.send(request).await.map_err(AppError::upstream)?;

    // The short URL's identity segment must be a *handle*: `resolve` parses it
    // with `Handle::new_owned`, and a DID is not a valid handle. Falling back to
    // the DID string therefore minted a URL that could never resolve, and
    // reported success while doing it.
    let Some(handle) = resolve_did_to_handle(
        &state.http,
        &state.resolver,
        &state.config.slingshot_url,
        &did_str,
    )
    .await
    else {
        tracing::error!(did = %did_str, "record written but handle resolution failed");
        return Err(AppError::Upstream(anyhow::anyhow!(
            "handle resolution failed for {did_str} after the record was written"
        )));
    };

    Ok(Json(ShortenResponse {
        short_url: state.config.base_url.short_url(&handle, &code),
    }))
}
// coverage:excl-stop

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_length() {
        for _ in 0..100 {
            let code = generate_code();
            assert_eq!(code.len(), 6);
            assert!(validate_code(&code));
        }
    }

    #[test]
    fn test_generate_code_charset() {
        for _ in 0..200 {
            let code = generate_code();
            assert!(
                code.chars().all(|c| c.is_ascii_alphanumeric()),
                "non-alphanumeric char in: {code}"
            );
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

    #[test]
    fn test_is_allowed_scheme() {
        assert!(is_allowed_scheme("https://example.com"));
        assert!(is_allowed_scheme("http://example.com/path?q=1"));
        assert!(!is_allowed_scheme("ftp://example.com/file.txt"));
        assert!(!is_allowed_scheme("javascript:void(0)"));
        assert!(!is_allowed_scheme("data:text/html,<h1>hi</h1>"));
        assert!(!is_allowed_scheme("not-a-valid-url"));
    }
}
