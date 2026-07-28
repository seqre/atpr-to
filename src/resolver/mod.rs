//! The read side: turning `@handle/code` into a destination.
//!
//! Two strategies behind one trait. `Slingshot` is two hops against a relay;
//! `Direct` is three hops against the user's own PDS and works when the relay
//! does not. `Chained` runs the first and falls back to the second.
//!
//! No dynamic dispatch here: tests swap the implementation by pointing
//! `slingshot_url` at a wiremock server, exactly as `tests/resolve_integration.rs`
//! already did, so `AppState` holds the concrete `Chained`.

pub mod direct;
pub mod slingshot;

use std::future::Future;

use jacquard_common::types::string::Handle;

use crate::domain::{InvalidTarget, ShortCode, ShortLink, TargetUrl};
use crate::error::AppError;

pub use direct::Direct;
pub use slingshot::Slingshot;

/// Why a resolution attempt failed.
///
/// This type exists because failures used to be classified with
/// `e.to_string().contains("404")`. `reqwest` appends ` for url ({url})` to the
/// Display of a send error, and the getRecord URL carries `rkey={code}` — so a
/// transport failure on a link named `promo404` was reported as "not found",
/// and the fallback was skipped because the caller believed the answer was
/// authoritative. Classifying from `resp.status()` at the point of the response
/// makes that misreading impossible.
#[derive(Debug)]
pub enum ResolveError {
    /// The handle does not resolve to a DID.
    HandleNotFound,
    /// The repo exists but holds no record under this short code.
    RecordNotFound,
    /// The record exists but its destination is not one we will redirect to —
    /// a `javascript:` URL, say. Anyone can write such a record to their own
    /// repo, so this is expected input, not an upstream fault.
    UnusableRecord(InvalidTarget),
    /// Transport failure, malformed response, or an upstream 5xx.
    Upstream(anyhow::Error),
}

impl ResolveError {
    /// True when there is no usable link at this address, and retrying or
    /// falling back to another resolver would not change that.
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::HandleNotFound | Self::RecordNotFound | Self::UnusableRecord(_)
        )
    }
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HandleNotFound => write!(f, "handle not found"),
            Self::RecordNotFound => write!(f, "record not found"),
            Self::UnusableRecord(e) => write!(f, "unusable record: {e}"),
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
            // 404 rather than 502: the record is permanently unusable, so
            // "try again later" would be a lie, and paging on it would be noise.
            ResolveError::UnusableRecord(reason) => {
                tracing::warn!(%reason, "record rejected on the read path");
                AppError::NotFound
            }
            ResolveError::Upstream(e) => AppError::Upstream(e),
        }
    }
}

/// Resolve a short link that anyone may request.
pub trait LinkResolver: Send + Sync {
    /// Look up the link `code` belonging to `handle`.
    fn resolve(
        &self,
        handle: &Handle,
        code: &ShortCode,
    ) -> impl Future<Output = Result<ShortLink, ResolveError>> + Send;
}

/// Pull a [`ShortLink`] out of a `com.atproto.repo.getRecord` response body.
///
/// Two byte-identical copies of this lived in `resolve.rs`, one per strategy.
///
/// The destination goes through `TargetUrl::parse` here, which is the point: a
/// repo is user-writable, so a record's `url` is attacker-controlled.
pub(crate) fn link_from_get_record(
    body: &serde_json::Value,
    source: &str,
) -> Result<ShortLink, ResolveError> {
    let missing =
        |field: &str| ResolveError::Upstream(anyhow::anyhow!("{source} getRecord missing {field}"));

    let value = body.get("value").ok_or_else(|| missing("value"))?;
    let raw_url = value
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| missing("url field"))?;

    let target = TargetUrl::parse(raw_url).map_err(ResolveError::UnusableRecord)?;
    let updated_at = value
        .get("updatedAt")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());

    Ok(ShortLink { target, updated_at })
}

/// Try the relay first, then the user's PDS.
pub struct Chained {
    /// The fast path.
    pub slingshot: Slingshot,
    /// The fallback.
    pub direct: Direct,
}

impl Chained {
    /// Build a chained resolver.
    pub fn new(slingshot: Slingshot, direct: Direct) -> Self {
        Self { slingshot, direct }
    }
}

impl LinkResolver for Chained {
    async fn resolve(&self, handle: &Handle, code: &ShortCode) -> Result<ShortLink, ResolveError> {
        let start = std::time::Instant::now();

        match self.slingshot.resolve(handle, code).await {
            Ok(link) => {
                tracing::info!(
                    path = "slingshot",
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "resolved"
                );
                Ok(link)
            }
            // A definitive not-found is authoritative. Falling back would only
            // add latency before reaching the same conclusion.
            Err(e) if e.is_not_found() => Err(e),
            // Anything else — including transport failures, which the old
            // string match could swallow — means the relay could not answer.
            Err(e) => {
                tracing::warn!(err = %e, "slingshot failed, falling back to direct");
                let link = self.direct.resolve(handle, code).await?;
                tracing::info!(
                    path = "direct",
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "resolved"
                );
                Ok(link)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn test_not_found_classification() {
        assert!(ResolveError::HandleNotFound.is_not_found());
        assert!(ResolveError::RecordNotFound.is_not_found());
        assert!(ResolveError::UnusableRecord(InvalidTarget::DisallowedScheme).is_not_found());
        assert!(!ResolveError::Upstream(anyhow::anyhow!("boom")).is_not_found());
    }

    /// The specific shape of bug #1: an upstream error whose text happens to
    /// contain "404" must not be classified as a not-found.
    #[test]
    fn test_upstream_mentioning_404_is_not_a_not_found() {
        let e = ResolveError::Upstream(anyhow::anyhow!(
            "error sending request for url (https://slingshot.example/xrpc/com.atproto.repo.getRecord?rkey=promo404)"
        ));
        assert!(!e.is_not_found());
        assert_eq!(AppError::from(e).status(), StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn test_status_mapping() {
        assert_eq!(
            AppError::from(ResolveError::RecordNotFound).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            AppError::from(ResolveError::UnusableRecord(
                InvalidTarget::DisallowedScheme
            ))
            .status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn test_link_from_get_record_rejects_dangerous_scheme() {
        let body = serde_json::json!({
            "value": { "url": "javascript:alert(1)", "updatedAt": "2024-01-01T00:00:00Z" }
        });
        let err = link_from_get_record(&body, "test").expect_err("must reject");
        assert!(matches!(
            err,
            ResolveError::UnusableRecord(InvalidTarget::DisallowedScheme)
        ));
    }

    #[test]
    fn test_link_from_get_record_happy_path() {
        let body = serde_json::json!({
            "value": { "url": "https://example.com/x", "updatedAt": "2024-01-01T00:00:00Z" }
        });
        let link = link_from_get_record(&body, "test").unwrap();
        assert_eq!(link.target.as_str(), "https://example.com/x");
        assert_eq!(link.updated_at.as_deref(), Some("2024-01-01T00:00:00Z"));
    }

    #[test]
    fn test_link_from_get_record_missing_fields() {
        let no_value = serde_json::json!({});
        assert!(matches!(
            link_from_get_record(&no_value, "test"),
            Err(ResolveError::Upstream(_))
        ));

        let no_url = serde_json::json!({ "value": { "updatedAt": "2024-01-01T00:00:00Z" } });
        assert!(matches!(
            link_from_get_record(&no_url, "test"),
            Err(ResolveError::Upstream(_))
        ));
    }
}
