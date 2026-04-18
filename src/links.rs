use jacquard::api::com_atproto::repo::list_records::ListRecords;
use jacquard_common::types::collection::Collection;
use jacquard_common::types::did::Did;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::xrpc::XrpcClient;

use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::auth::AuthSession;
use crate::error;
use crate::generated::to_atpr::link::Link;

/// Extract the record key (rkey) from an AT-URI string.
///
/// AT-URI format: `at://{did}/{collection}/{rkey}`
pub fn rkey_from_at_uri(at_uri: &str) -> &str {
    at_uri.split('/').next_back().unwrap_or("")
}

/// List the authenticated user's shortened links.
///
/// Calls `com.atproto.repo.listRecords` on the user's PDS and returns a JSON object
/// with a `links` array of `{ code, url, created_at, expires_at }` entries.
#[tracing::instrument(skip_all)]
// coverage:excl-start
pub async fn list_links(auth: AuthSession) -> Response {
    let AuthSession(session) = auth;
    let (did, _) = session.session_info().await;
    let did_str = did.as_ref().to_string();

    let owned_did: Did = match Did::new_owned(&did_str) {
        Ok(d) => d,
        Err(_) => return error::unauthorized("Invalid DID in session"),
    };

    let collection = Nsid::new_static(<Link as Collection>::NSID).expect("valid NSID");

    let request = ListRecords::new()
        .repo(AtIdentifier::Did(owned_did))
        .collection(collection)
        .build();

    let raw_response = match session.send(request).await {
        Ok(r) => r,
        Err(e) => return error::internal_error(&format!("Failed to list records: {e}")),
    };

    let output = match raw_response.into_output() {
        Ok(o) => o,
        Err(e) => {
            return error::internal_error(&format!("Failed to parse listRecords response: {e}"))
        }
    };

    let links: Vec<serde_json::Value> = output
        .records
        .iter()
        .filter_map(|record| {
            let code = rkey_from_at_uri(record.uri.as_ref()).to_string();
            let value = match serde_json::to_value(&record.value) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(%e, "failed to serialize record");
                    return None;
                }
            };
            let url = value
                .get("url")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string();
            let updated_at = value
                .get("updatedAt")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            Some(serde_json::json!({
                "code": code,
                "url": url,
                "updated_at": updated_at,
            }))
        })
        .collect();

    Json(serde_json::json!({ "links": links })).into_response()
}
// coverage:excl-stop

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;
    use crate::router;

    #[test]
    fn test_rkey_from_at_uri() {
        assert_eq!(
            rkey_from_at_uri("at://did:plc:abc123/to.atpr.link/mycode"),
            "mycode"
        );
        assert_eq!(
            rkey_from_at_uri("at://did:plc:abc123/to.atpr.link/abc-123_XY"),
            "abc-123_XY"
        );
        assert_eq!(rkey_from_at_uri(""), "");
    }

    #[tokio::test]
    async fn test_links_requires_auth() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/links")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
