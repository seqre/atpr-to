use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use jacquard::identity::resolver::IdentityResolver;
use jacquard::identity::JacquardResolver;
use jacquard_common::types::string::Handle;

/// Resolve a short URL and redirect.
///
/// Flow: handle → DID → PDS endpoint → getRecord → extract URL → 302 redirect
pub async fn resolve(Path((handle, code)): Path<(String, String)>) -> Response {
    // Parse and validate handle
    let handle = match Handle::new(&handle) {
        Ok(h) => h,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("Invalid handle: {e}"),
            )
                .into_response();
        }
    };

    // Resolve handle → DID
    let resolver = JacquardResolver::default();
    let did = match resolver.resolve_handle(&handle).await {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                format!("Could not resolve handle: {e}"),
            )
                .into_response();
        }
    };

    // Resolve DID → DID document → PDS endpoint
    let doc_response = match resolver.resolve_did_doc(&did).await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Could not resolve DID document: {e}"),
            )
                .into_response();
        }
    };

    let doc = match doc_response.parse() {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Could not parse DID document: {e}"),
            )
                .into_response();
        }
    };

    let pds_url = match doc.pds_endpoint() {
        Some(u) => u,
        None => {
            return (
                StatusCode::BAD_GATEWAY,
                "No PDS endpoint found in DID document",
            )
                .into_response();
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
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to fetch record from PDS: {e}"),
            )
                .into_response();
        }
    };

    if !resp.status().is_success() {
        return (
            StatusCode::NOT_FOUND,
            format!("Record not found (PDS returned {})", resp.status()),
        )
            .into_response();
    }

    // Parse the getRecord response — we need the "value" field which contains our Link record
    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to parse PDS response: {e}"),
            )
                .into_response();
        }
    };

    let url = match body.get("value").and_then(|v| v.get("url")).and_then(|u| u.as_str()) {
        Some(u) => u.to_string(),
        None => {
            return (
                StatusCode::BAD_GATEWAY,
                "Record missing url field",
            )
                .into_response();
        }
    };

    Redirect::temporary(&url).into_response()
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
}
