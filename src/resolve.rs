use axum::extract::Path;
use axum::response::{IntoResponse, Redirect, Response};
use jacquard::identity::resolver::IdentityResolver;
use jacquard::identity::JacquardResolver;
use jacquard_common::types::string::Handle;

use crate::error;

/// Resolve a short URL and redirect.
///
/// Flow: handle → DID → PDS endpoint → getRecord → extract URL → 302 redirect
pub async fn resolve(Path((handle, code)): Path<(String, String)>) -> Response {
    // Parse and validate handle
    let handle = match Handle::new(&handle) {
        Ok(h) => h,
        Err(e) => {
            return error::bad_request(&format!("Invalid handle: {e}"));
        }
    };

    // Resolve handle → DID
    let resolver = JacquardResolver::default();
    let did = match resolver.resolve_handle(&handle).await {
        Ok(d) => d,
        Err(e) => {
            return error::not_found(&format!("Could not resolve handle: {e}"));
        }
    };

    // Resolve DID → DID document → PDS endpoint
    let doc_response = match resolver.resolve_did_doc(&did).await {
        Ok(r) => r,
        Err(e) => {
            return error::bad_gateway(&format!("Could not resolve DID document: {e}"));
        }
    };

    let doc = match doc_response.parse() {
        Ok(d) => d,
        Err(e) => {
            return error::bad_gateway(&format!("Could not parse DID document: {e}"));
        }
    };

    let pds_url = match doc.pds_endpoint() {
        Some(u) => u,
        None => {
            return error::bad_gateway("No PDS endpoint found in DID document");
        }
    };

    // Fetch record from PDS (public, unauthenticated)
    let get_record_url = format!(
        "{}xrpc/com.atproto.repo.getRecord?repo={}&collection=to.atpr.link&rkey={}",
        pds_url,
        urlencoding::encode(did.as_ref()),
        urlencoding::encode(&code),
    );

    let client = reqwest::Client::new();
    let resp = match client.get(&get_record_url).send().await {
        Ok(r) => r,
        Err(e) => {
            return error::bad_gateway(&format!("Failed to fetch record from PDS: {e}"));
        }
    };

    if !resp.status().is_success() {
        return error::not_found(&format!("Record not found (PDS returned {})", resp.status()));
    }

    // Parse the getRecord response — we need the "value" field which contains our Link record
    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return error::bad_gateway(&format!("Failed to parse PDS response: {e}"));
        }
    };

    let url = match body
        .get("value")
        .and_then(|v| v.get("url"))
        .and_then(|u| u.as_str())
    {
        Some(u) => u.to_string(),
        None => {
            return error::bad_gateway("Record missing url field");
        }
    };

    Redirect::temporary(&url).into_response()
}

#[cfg(test)]
mod tests {
    use jacquard_common::types::string::Handle;

    #[test]
    fn test_handle_parsing() {
        assert!(Handle::new("alice.bsky.social").is_ok());
        assert!(Handle::new("seqre.dev").is_ok());
        // Single-label handles are invalid per AT Protocol
        assert!(Handle::new("invalid").is_err());
    }
}
