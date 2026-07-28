use axum::extract::Path;
use axum::http::StatusCode;
use jacquard::api::com_atproto::repo::delete_record::DeleteRecord;
use jacquard_common::types::collection::Collection;
use jacquard_common::types::did::Did;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::types::recordkey::{RecordKey, Rkey};
use jacquard_common::xrpc::XrpcClient;

use crate::auth::AuthSession;
use crate::error::AppError;
use crate::generated::to_atpr::link::Link;
use crate::shorten::validate_code;

/// Delete a short URL record. Requires authentication.
#[tracing::instrument(skip_all, fields(code))]
// coverage:excl-start
pub async fn delete_link(
    auth: AuthSession,
    Path(code): Path<String>,
) -> Result<StatusCode, AppError> {
    let AuthSession(session) = auth;
    let (did, _) = session.session_info().await;
    let did_str = did.as_ref().to_string();

    if !validate_code(&code) {
        return Err(AppError::BadRequest("Invalid code"));
    }

    let rkey: RecordKey<Rkey> =
        RecordKey::any_owned(&code).map_err(|_| AppError::BadRequest("Invalid code"))?;
    let owned_did: Did = Did::new_owned(&did_str).map_err(|_| AppError::Unauthorized)?;
    let collection = Nsid::new_static(<Link as Collection>::NSID).expect("valid NSID");

    let request = DeleteRecord::new()
        .repo(AtIdentifier::Did(owned_did))
        .collection(collection)
        .rkey(rkey)
        .build();

    session.send(request).await.map_err(AppError::upstream)?;
    Ok(StatusCode::NO_CONTENT)
}
// coverage:excl-stop
