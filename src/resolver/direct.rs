//! Resolution straight from the user's PDS: handle → DID → DID doc → getRecord.
//!
//! Slower than the relay, but it works when the relay does not, and it is the
//! only path that does not depend on a third party.

use jacquard::identity::resolver::{IdentityError, IdentityErrorKind, IdentityResolver};
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

/// Classify a failure to turn a handle into a DID.
///
/// Every identity failure used to be an upstream fault, so a visitor who
/// mistyped a handle — or followed a link to an account that no longer exists —
/// was told the network was not answering, and got a 502 for a request that was
/// never going to succeed.
///
/// `HandleResolutionExhausted` is jacquard's verdict after DNS, the well-known
/// document and the PDS fallback have all been tried. Its own help text calls
/// it ambiguous: the handle may not exist, *or* every method may be
/// unreachable. Treated here as "no such account", because a typo is the
/// overwhelmingly common cause and a simultaneous failure of all three methods
/// means this service is not resolving anything for anyone. Every other kind
/// stays an upstream fault.
///
/// Matched on the typed kind, never on the message — the same rule the rest of
/// this module follows, and the reason `ResolveError` exists at all.
fn classify_handle_failure(e: IdentityError) -> ResolveError {
    if matches!(e.kind(), IdentityErrorKind::HandleResolutionExhausted) {
        ResolveError::HandleNotFound
    } else {
        ResolveError::Upstream(e.into())
    }
}

/// Build the `getRecord` URL for a PDS endpoint taken from a DID document.
///
/// Split out and given its own tests because the endpoint is a value from a
/// third party and its shape is not ours to assume. It was interpolated as
/// `{pds_url}xrpc/…`, which is only correct when the DID document happens to
/// carry a trailing slash. Almost none do — `https://pds.rip` produced
/// `https://pds.ripxrpc/…`, a hostname that is not the PDS and, on a wildcard
/// domain, one that answers just enough to fail late. Every direct resolution
/// against a real PDS ended as a transport error, so the fallback the whole
/// resolver chain exists for had never once worked; the only reason nothing
/// caught it is that the integration tests reach this path expecting it to
/// fail for want of a routable PDS, and got the failure they expected.
fn get_record_url(pds_url: &str, did: &str, code: &ShortCode) -> String {
    format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection=to.atpr.link&rkey={}",
        pds_url.trim_end_matches('/'),
        urlencoding::encode(did),
        urlencoding::encode(code.as_str()),
    )
}

impl LinkResolver for Direct {
    async fn resolve(&self, handle: &Handle, code: &ShortCode) -> Result<ShortLink, ResolveError> {
        async {
            let did = async { self.identity.resolve_handle(handle).await }
                .instrument(tracing::info_span!("resolve_handle"))
                .await
                .map_err(classify_handle_failure)?;

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

            let get_record_url = get_record_url(pds_url.as_ref(), did.as_ref(), code);

            let resp = async { self.http.get(&get_record_url).send().await }
                .instrument(tracing::info_span!("fetch_record"))
                .await?;

            if !resp.status().is_success() {
                return Err(super::getrecord_failure("PDS", resp).await);
            }

            link_from_get_record(&resp.json().await?, "PDS")
        }
        .instrument(tracing::info_span!("direct"))
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DID: &str = "did:plc:erpkpcbofe5zdtbmdc3mx7fo";

    fn code(s: &str) -> ShortCode {
        ShortCode::parse(s).unwrap()
    }

    /// The shape almost every DID document actually uses.
    #[test]
    fn test_endpoint_without_a_trailing_slash_still_gets_one_separator() {
        let url = get_record_url("https://pds.rip", DID, &code("gridsmith"));
        assert!(
            url.starts_with("https://pds.rip/xrpc/com.atproto.repo.getRecord?"),
            "the host must survive the join: {url}"
        );
    }

    /// And the shape that used to be assumed must not now produce `//xrpc`.
    #[test]
    fn test_endpoint_with_a_trailing_slash_does_not_double_it() {
        let url = get_record_url("https://pds.rip/", DID, &code("gridsmith"));
        assert!(url.starts_with("https://pds.rip/xrpc/"), "{url}");
    }

    #[test]
    fn test_repo_and_rkey_are_encoded() {
        let url = get_record_url("https://pds.rip", DID, &code("a-b_c"));
        assert!(
            url.contains("repo=did%3Aplc%3Aerpkpcbofe5zdtbmdc3mx7fo"),
            "{url}"
        );
        assert!(url.ends_with("&rkey=a-b_c"), "{url}");
        assert!(url.contains("&collection=to.atpr.link&"), "{url}");
    }
}
