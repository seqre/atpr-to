use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use jacquard::identity::resolver::IdentityResolver;
use jacquard_common::types::string::Handle;
use tracing::Instrument;

use crate::auth::Resolver;
use crate::error::AppError;
use crate::AppState;

/// A successfully resolved short link.
pub(crate) struct ResolvedLink {
    /// The destination URL to redirect to.
    pub url: String,
    /// Last-modified datetime string (ISO 8601).
    pub updated_at: Option<String>,
}

/// Why a resolution attempt failed.
///
/// This exists because the previous code classified failures with
/// `e.to_string().contains("404")`. The formatted error embeds the request URL,
/// and the request URL embeds the user's short code — so a link named `promo404`
/// was reported as "not found" on a *transport* error, and, worse, the direct
/// PDS fallback was skipped entirely because the caller believed the answer was
/// authoritative. Classifying from `resp.status()` at the point of the response
/// makes that misreading impossible.
#[derive(Debug)]
pub(crate) enum ResolveError {
    /// The handle does not resolve to a DID.
    HandleNotFound,
    /// The repo exists but holds no record under this short code.
    RecordNotFound,
    /// Transport failure, malformed response, or an upstream 5xx.
    Upstream(anyhow::Error),
}

impl ResolveError {
    /// True when the upstream gave a definitive "this does not exist".
    ///
    /// Only these justify skipping the fallback path.
    pub(crate) fn is_not_found(&self) -> bool {
        matches!(self, Self::HandleNotFound | Self::RecordNotFound)
    }
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HandleNotFound => write!(f, "handle not found"),
            Self::RecordNotFound => write!(f, "record not found"),
            Self::Upstream(e) => write!(f, "{e}"),
        }
    }
}

impl From<reqwest::Error> for ResolveError {
    fn from(e: reqwest::Error) -> Self {
        Self::Upstream(e.into())
    }
}

impl From<ResolveError> for AppError {
    fn from(e: ResolveError) -> Self {
        match e {
            ResolveError::HandleNotFound | ResolveError::RecordNotFound => AppError::NotFound,
            ResolveError::Upstream(e) => AppError::Upstream(e),
        }
    }
}

/// Pull the link fields out of a `com.atproto.repo.getRecord` response body.
fn link_from_get_record(
    body: &serde_json::Value,
    source: &str,
) -> Result<ResolvedLink, ResolveError> {
    let missing =
        |field: &str| ResolveError::Upstream(anyhow::anyhow!("{source} getRecord missing {field}"));

    let value = body.get("value").ok_or_else(|| missing("value"))?;
    let url = value
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| missing("url field"))?
        .to_string();
    let updated_at = value
        .get("updatedAt")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());

    Ok(ResolvedLink { url, updated_at })
}

/// Try to resolve via Slingshot (2-hop: resolveHandle + getRecord).
pub(crate) async fn resolve_via_slingshot(
    client: &reqwest::Client,
    slingshot_url: &str,
    handle: &str,
    code: &str,
) -> Result<ResolvedLink, ResolveError> {
    let base = slingshot_url.trim_end_matches('/');

    // Hop 1: resolveHandle
    let resolve_url = format!(
        "{}/xrpc/com.atproto.identity.resolveHandle?handle={}",
        base,
        urlencoding::encode(handle),
    );
    let resp = client.get(&resolve_url).send().await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(ResolveError::HandleNotFound);
    }
    if !resp.status().is_success() {
        return Err(ResolveError::Upstream(anyhow::anyhow!(
            "Slingshot resolveHandle returned {}",
            resp.status()
        )));
    }
    let body: serde_json::Value = resp.json().await?;
    let did = body
        .get("did")
        .and_then(|d| d.as_str())
        .ok_or_else(|| {
            ResolveError::Upstream(anyhow::anyhow!("Slingshot resolveHandle missing did field"))
        })?
        .to_string();

    // Hop 2: getRecord
    let record_url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection=to.atpr.link&rkey={}",
        base,
        urlencoding::encode(&did),
        urlencoding::encode(code),
    );
    let resp = client.get(&record_url).send().await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(ResolveError::RecordNotFound);
    }
    if !resp.status().is_success() {
        return Err(ResolveError::Upstream(anyhow::anyhow!(
            "Slingshot getRecord returned {}",
            resp.status()
        )));
    }

    link_from_get_record(&resp.json().await?, "Slingshot")
}

/// Resolve via direct 3-hop path: handle → DID → DID doc → PDS getRecord.
// coverage:excl-start
async fn resolve_via_direct(
    client: &reqwest::Client,
    resolver: &Resolver,
    handle: &Handle,
    code: &str,
) -> Result<ResolvedLink, ResolveError> {
    let did = async { resolver.resolve_handle(handle).await }
        .instrument(tracing::info_span!("resolve_handle"))
        .await
        .map_err(|e| ResolveError::Upstream(e.into()))?;

    let doc_response = async { resolver.resolve_did_doc(&did).await }
        .instrument(tracing::info_span!("resolve_did_doc"))
        .await
        .map_err(|e| ResolveError::Upstream(e.into()))?;

    let doc = doc_response
        .parse()
        .map_err(|e| ResolveError::Upstream(anyhow::Error::from(e)))?;

    let pds_url = doc.pds_endpoint().ok_or_else(|| {
        ResolveError::Upstream(anyhow::anyhow!("No PDS endpoint in DID document"))
    })?;

    let get_record_url = format!(
        "{}xrpc/com.atproto.repo.getRecord?repo={}&collection=to.atpr.link&rkey={}",
        pds_url,
        urlencoding::encode(did.as_ref()),
        urlencoding::encode(code),
    );

    let resp = async { client.get(&get_record_url).send().await }
        .instrument(tracing::info_span!("fetch_record"))
        .await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(ResolveError::RecordNotFound);
    }
    if !resp.status().is_success() {
        return Err(ResolveError::Upstream(anyhow::anyhow!(
            "PDS getRecord returned {}",
            resp.status()
        )));
    }

    link_from_get_record(&resp.json().await?, "PDS")
}
// coverage:excl-stop

/// Resolve a short URL and redirect.
///
/// Tries Slingshot first (2-hop), falls back to direct resolution (3-hop) on any error.
#[tracing::instrument(skip(state), fields(handle, code))]
pub async fn resolve(
    State(state): State<Arc<AppState>>,
    Path((handle, code)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let start = std::time::Instant::now();

    let parsed_handle: Handle =
        Handle::new_owned(&handle).map_err(|_| AppError::BadRequest("Invalid handle"))?;

    // Try Slingshot first
    let link = match async {
        resolve_via_slingshot(&state.http, &state.config.slingshot_url, &handle, &code).await
    }
    .instrument(tracing::info_span!("slingshot"))
    .await
    {
        Ok(link) => {
            tracing::info!(
                path = "slingshot",
                elapsed_ms = start.elapsed().as_millis() as u64,
                "resolved"
            );
            link
        }
        // A 404 from Slingshot is authoritative — the record doesn't exist.
        // Don't fall back; that would only add latency before the same conclusion.
        Err(e) if e.is_not_found() => return Err(AppError::NotFound),
        // Anything else — including transport failures, which the old string
        // match could swallow — means Slingshot could not answer. Fall back.
        Err(slingshot_err) => {
            tracing::warn!(err = %slingshot_err, "slingshot failed, falling back to direct");
            async { resolve_via_direct(&state.http, &state.resolver, &parsed_handle, &code).await }
                .instrument(tracing::info_span!("direct"))
                .await
                .inspect(|_| {
                    tracing::info!(
                        path = "direct",
                        elapsed_ms = start.elapsed().as_millis() as u64,
                        "resolved"
                    );
                })?
        }
    };

    Ok(Redirect::temporary(&link.url).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_parsing() {
        assert!(
            Handle::<jacquard_common::deps::smol_str::SmolStr>::new_owned("alice.bsky.social")
                .is_ok()
        );
        assert!(Handle::<jacquard_common::deps::smol_str::SmolStr>::new_owned("seqre.dev").is_ok());
        // Single-label handles are invalid per AT Protocol
        assert!(Handle::<jacquard_common::deps::smol_str::SmolStr>::new_owned("invalid").is_err());
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
