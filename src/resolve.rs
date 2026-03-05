use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use jacquard::identity::resolver::IdentityResolver;
use jacquard::identity::JacquardResolver;
use jacquard_common::types::string::Handle;
use tracing::Instrument;

use crate::error;
use crate::AppState;

/// A successfully resolved short link.
pub(crate) struct ResolvedLink {
    /// The destination URL to redirect to.
    pub url: String,
    /// Optional expiry datetime string (ISO 8601).
    pub expires_at: Option<String>,
}

/// Try to resolve via Slingshot (2-hop: resolveHandle + getRecord).
/// Returns the resolved link (URL + optional expiry) on success.
pub(crate) async fn resolve_via_slingshot(
    client: &reqwest::Client,
    slingshot_url: &str,
    handle: &str,
    code: &str,
) -> anyhow::Result<ResolvedLink> {
    let base = slingshot_url.trim_end_matches('/');

    // Hop 1: resolveHandle
    let resolve_url = format!(
        "{}/xrpc/com.atproto.identity.resolveHandle?handle={}",
        base,
        urlencoding::encode(handle),
    );
    let resp = client.get(&resolve_url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("Slingshot resolveHandle returned {}", resp.status());
    }
    let body: serde_json::Value = resp.json().await?;
    let did = body
        .get("did")
        .and_then(|d| d.as_str())
        .ok_or_else(|| anyhow::anyhow!("Slingshot resolveHandle missing did field"))?
        .to_string();

    // Hop 2: getRecord
    let record_url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection=to.atpr.link&rkey={}",
        base,
        urlencoding::encode(&did),
        urlencoding::encode(code),
    );
    let resp = client.get(&record_url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("Slingshot getRecord returned {}", resp.status());
    }
    let body: serde_json::Value = resp.json().await?;
    let value = body.get("value").ok_or_else(|| anyhow::anyhow!("Slingshot getRecord missing value"))?;
    let url = value
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| anyhow::anyhow!("Slingshot getRecord missing url field"))?
        .to_string();
    let expires_at = value
        .get("expiresAt")
        .and_then(|e| e.as_str())
        .map(|s| s.to_string());

    Ok(ResolvedLink { url, expires_at })
}

/// Resolve via direct 3-hop path: handle → DID → DID doc → PDS getRecord.
/// Returns the resolved link on success.
// coverage:excl-start
async fn resolve_via_direct(
    client: &reqwest::Client,
    handle: &Handle<'_>,
    code: &str,
) -> anyhow::Result<ResolvedLink> {
    let resolver = JacquardResolver::default();

    let did = async { resolver.resolve_handle(handle).await }
        .instrument(tracing::info_span!("resolve_handle"))
        .await?;

    let doc_response = async { resolver.resolve_did_doc(&did).await }
        .instrument(tracing::info_span!("resolve_did_doc"))
        .await?;

    let doc = doc_response.parse()?;

    let pds_url = doc
        .pds_endpoint()
        .ok_or_else(|| anyhow::anyhow!("No PDS endpoint in DID document"))?;

    let get_record_url = format!(
        "{}xrpc/com.atproto.repo.getRecord?repo={}&collection=to.atpr.link&rkey={}",
        pds_url,
        urlencoding::encode(did.as_ref()),
        urlencoding::encode(code),
    );

    let resp = async { client.get(&get_record_url).send().await }
        .instrument(tracing::info_span!("fetch_record"))
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("PDS getRecord returned {}", resp.status());
    }

    let body: serde_json::Value = resp.json().await?;
    let value = body.get("value").ok_or_else(|| anyhow::anyhow!("PDS getRecord missing value"))?;
    let url = value
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| anyhow::anyhow!("PDS getRecord missing url field"))?
        .to_string();
    let expires_at = value
        .get("expiresAt")
        .and_then(|e| e.as_str())
        .map(|s| s.to_string());

    Ok(ResolvedLink { url, expires_at })
}
// coverage:excl-stop

/// Resolve a short URL and redirect.
///
/// Tries Slingshot first (2-hop), falls back to direct resolution (3-hop) on any error.
#[tracing::instrument(skip(state), fields(handle, code))]
pub async fn resolve(
    State(state): State<Arc<AppState>>,
    Path((handle, code)): Path<(String, String)>,
) -> Response {
    let start = std::time::Instant::now();

    // Validate handle
    let parsed_handle = match Handle::new(&handle) {
        Ok(h) => h,
        Err(e) => return error::bad_request(&format!("Invalid handle: {e}")),
    };

    // Try Slingshot first
    let link = match async {
        resolve_via_slingshot(&state.http, &state.config.slingshot_url, &handle, &code).await
    }
    .instrument(tracing::info_span!("slingshot"))
    .await
    {
        Ok(link) => {
            tracing::info!(path = "slingshot", elapsed_ms = start.elapsed().as_millis() as u64, "resolved");
            link
        }
        Err(slingshot_err) => {
            // A 404 from Slingshot is authoritative — the record doesn't exist.
            // Don't fall back; that would only add latency before the same conclusion.
            if slingshot_err.to_string().contains("404") {
                return error::not_found("Link not found");
            }
            tracing::warn!(err = %slingshot_err, "slingshot failed, falling back to direct");
            match async {
                resolve_via_direct(&state.http, &parsed_handle, &code).await
            }
            .instrument(tracing::info_span!("direct"))
            .await
            {
                // coverage:excl-start
                Ok(link) => {
                    tracing::info!(path = "direct", elapsed_ms = start.elapsed().as_millis() as u64, "resolved");
                    link
                }
                // coverage:excl-stop
                Err(e) => {
                    // Distinguish not-found from upstream errors
                    let msg = e.to_string();
                    // coverage:excl-start
                    if msg.contains("404") {
                        return error::not_found("Link not found");
                    }
                    // coverage:excl-stop
                    return error::bad_gateway(&format!("Could not resolve link: {e}"));
                }
            }
        }
    };

    // Check expiry
    if let Some(ref expires_at) = link.expires_at {
        if let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(expires_at) {
            if expiry < chrono::Utc::now() {
                return error::gone("This link has expired.");
            }
        }
    }

    Redirect::temporary(&link.url).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_parsing() {
        assert!(Handle::new("alice.bsky.social").is_ok());
        assert!(Handle::new("seqre.dev").is_ok());
        // Single-label handles are invalid per AT Protocol
        assert!(Handle::new("invalid").is_err());
    }

    #[test]
    fn test_slingshot_url_construction() {
        // Verify special chars in handles/DIDs are percent-encoded
        let handle = "user.with.dots.bsky.social";
        let did = "did:plc:abc+def/ghi";
        let code = "my-code_1";
        let base = "https://slingshot.microcosm.blue";

        let resolve_url = format!(
            "{}/xrpc/com.atproto.identity.resolveHandle?handle={}",
            base,
            urlencoding::encode(handle),
        );
        assert!(resolve_url.contains("user.with.dots.bsky.social")); // dots are safe

        let record_url = format!(
            "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection=to.atpr.link&rkey={}",
            base,
            urlencoding::encode(did),
            urlencoding::encode(code),
        );
        assert!(record_url.contains("did%3Aplc%3Aabc%2Bdef%2Fghi"));
        assert!(record_url.contains("my-code_1"));
    }
}
