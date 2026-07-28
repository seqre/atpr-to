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
use serde::{Deserialize, Serialize};

use crate::auth::{AuthSession, Resolver};
use crate::domain::{ShortCode, TargetUrl};
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

    // Four separate validation steps and three parses of `body.url` collapse
    // into two constructors that either produce a valid value or say why not.
    let code = match &body.code {
        Some(c) => ShortCode::parse(c)?,
        None => ShortCode::generate(),
    };
    let target = TargetUrl::parse(&body.url)?;

    let link_url: UriValue = UriValue::new_owned(target.as_str()).map_err(AppError::internal)?;

    let record: Link = Link::new()
        .url(link_url)
        .updated_at(Datetime::now())
        .build();

    let data = to_data(&record).map_err(AppError::internal)?;
    // `ShortCode`'s charset is a subset of the atproto rkey charset, so this
    // branch is unreachable; it is an internal error rather than a 400 because
    // reaching it would mean the two have drifted apart.
    let rkey: RecordKey<Rkey> = RecordKey::any_owned(code.as_str()).map_err(AppError::internal)?;
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
        short_url: state.config.base_url.short_url(&handle, code.as_str()),
    }))
}
// coverage:excl-stop
