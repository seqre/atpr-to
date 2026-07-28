//! Resolution straight from the user's PDS: handle → DID → DID doc → getRecord.
//!
//! Slower than the relay, but it works when the relay does not, and it is the
//! only path that does not depend on a third party.

use jacquard::identity::resolver::IdentityResolver;
use jacquard_common::types::string::Handle;
use tracing::Instrument;

use super::{link_from_get_record, LinkResolver, ResolveError};
use crate::auth::Resolver;
use crate::domain::{ShortCode, ShortLink};

/// Resolve against the user's own PDS.
pub struct Direct {
    http: reqwest::Client,
    identity: Resolver,
}

impl Direct {
    /// Build a direct resolver.
    pub fn new(http: reqwest::Client, identity: Resolver) -> Self {
        Self { http, identity }
    }
}

impl LinkResolver for Direct {
    async fn resolve(&self, handle: &Handle, code: &ShortCode) -> Result<ShortLink, ResolveError> {
        async {
            let did = async { self.identity.resolve_handle(handle).await }
                .instrument(tracing::info_span!("resolve_handle"))
                .await
                .map_err(|e| ResolveError::Upstream(e.into()))?;

            let doc_response = async { self.identity.resolve_did_doc(&did).await }
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
                urlencoding::encode(code.as_str()),
            );

            let resp = async { self.http.get(&get_record_url).send().await }
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
        .instrument(tracing::info_span!("direct"))
        .await
    }
}
