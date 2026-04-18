use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use jacquard::api::com_atproto::repo::delete_record::DeleteRecord;
use jacquard_common::types::collection::Collection;
use jacquard_common::types::did::Did;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::types::recordkey::{RecordKey, Rkey};
use jacquard_common::xrpc::XrpcClient;

use crate::auth::AuthSession;
use crate::generated::to_atpr::link::Link;
use crate::shorten::validate_code;

/// Delete a short URL record. Requires authentication.
#[tracing::instrument(skip_all, fields(code))]
// coverage:excl-start
pub async fn delete_link(auth: AuthSession, Path(code): Path<String>) -> Response {
    let AuthSession(session) = auth;
    let (did, _) = session.session_info().await;
    let did_str = did.as_ref().to_string();

    // Validate code
    if !validate_code(&code) {
        return (StatusCode::BAD_REQUEST, "Invalid code").into_response();
    }

    // Build the rkey
    let rkey: RecordKey<Rkey> = match RecordKey::any_owned(&code) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid record key: {e}")).into_response();
        }
    };

    let owned_did: Did = match Did::new_owned(&did_str) {
        Ok(d) => d,
        Err(_) => return (StatusCode::UNAUTHORIZED, "Invalid DID in session").into_response(),
    };

    let collection = Nsid::new_static(<Link as Collection>::NSID).expect("valid NSID");

    // Build DeleteRecord request
    let request = DeleteRecord::new()
        .repo(AtIdentifier::Did(owned_did))
        .collection(collection)
        .rkey(rkey)
        .build();

    match session.send(request).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete record: {e}"),
        )
            .into_response(),
    }
}
// coverage:excl-stop
