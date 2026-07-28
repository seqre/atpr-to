//! Resolution via a Slingshot relay: two hops, `resolveHandle` then `getRecord`.

use jacquard_common::types::string::Handle;
use tracing::Instrument;

use super::{link_from_get_record, LinkResolver, ResolveError};
use crate::domain::{ShortCode, ShortLink};

/// Resolve through a Slingshot relay.
pub struct Slingshot {
    http: reqwest::Client,
    /// Relay origin, without a trailing slash.
    base: String,
}

impl Slingshot {
    /// Build a resolver against the relay at `base`.
    pub fn new(http: reqwest::Client, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_string(),
        }
    }

    /// The relay origin this resolver targets.
    pub fn base(&self) -> &str {
        &self.base
    }
}

impl LinkResolver for Slingshot {
    async fn resolve(&self, handle: &Handle, code: &ShortCode) -> Result<ShortLink, ResolveError> {
        async {
            // Hop 1: handle → DID
            let resolve_url = format!(
                "{}/xrpc/com.atproto.identity.resolveHandle?handle={}",
                self.base,
                urlencoding::encode(handle.as_ref()),
            );
            let resp = self.http.get(&resolve_url).send().await?;
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
                    ResolveError::Upstream(anyhow::anyhow!(
                        "Slingshot resolveHandle missing did field"
                    ))
                })?
                .to_string();

            // Hop 2: DID + code → record
            let record_url = format!(
                "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection=to.atpr.link&rkey={}",
                self.base,
                urlencoding::encode(&did),
                urlencoding::encode(code.as_str()),
            );
            let resp = self.http.get(&record_url).send().await?;
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
        .instrument(tracing::info_span!("slingshot"))
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_is_normalised() {
        let r = Slingshot::new(reqwest::Client::new(), "https://relay.example/");
        assert_eq!(r.base(), "https://relay.example");
        let r = Slingshot::new(reqwest::Client::new(), "https://relay.example");
        assert_eq!(r.base(), "https://relay.example");
    }
}
