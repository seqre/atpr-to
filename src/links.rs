use jacquard::api::com_atproto::repo::list_records::ListRecords;
use jacquard_common::types::collection::Collection;
use jacquard_common::types::did::Did;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::xrpc::XrpcClient;

use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::auth::AuthSession;
use crate::error::AppError;
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
pub async fn list_links(auth: AuthSession) -> Result<Response, AppError> {
    let AuthSession(session) = auth;
    let (did, _) = session.session_info().await;
    let did_str = did.as_ref().to_string();

    let owned_did: Did = Did::new_owned(&did_str).map_err(|_| AppError::Unauthorized)?;

    let collection = Nsid::new_static(<Link as Collection>::NSID).expect("valid NSID");

    let request = ListRecords::new()
        .repo(AtIdentifier::Did(owned_did))
        .collection(collection)
        .build();

    let raw_response = session.send(request).await.map_err(AppError::upstream)?;
    let output = raw_response.into_output().map_err(AppError::upstream)?;

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

    Ok(Json(serde_json::json!({ "links": links })).into_response())
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

    /// A JSON endpoint must not answer errors in `text/plain`. It used to:
    /// the auth rejection and every `error::*` helper emitted plain text, which
    /// `templates/dashboard.html` already flagged as a trap for any client that
    /// parses the body.
    #[tokio::test]
    async fn test_links_error_body_is_json() {
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

        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).expect("body must be JSON");
        assert_eq!(json["error"], "unauthorized");
    }
}
