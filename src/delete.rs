use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use jacquard::api::com_atproto::repo::delete_record::DeleteRecord;
use jacquard_common::types::collection::Collection;
use jacquard_common::types::did::Did;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::xrpc::XrpcClient;

use crate::auth::parse_session_cookie;
use crate::generated::to_atpr::link::Link;
use crate::shorten::validate_code;
use crate::AppState;

/// Delete a short URL record. Requires authentication.
#[tracing::instrument(skip_all, fields(code))]
pub async fn delete_link(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(code): Path<String>,
) -> Response {
    // Auth check
    let Some((did_str, session_id)) = parse_session_cookie(&jar) else {
        return (StatusCode::UNAUTHORIZED, "Authentication required").into_response();
    };

    // Validate code
    if !validate_code(&code) {
        return (StatusCode::BAD_REQUEST, "Invalid code").into_response();
    }

    // Restore OAuth session
    let did = match Did::new(&did_str) {
        Ok(d) => d,
        Err(_) => return (StatusCode::UNAUTHORIZED, "Invalid session").into_response(),
    };

    let session = match state.oauth.restore(&did, &session_id).await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::UNAUTHORIZED,
                format!("Session expired: {e}"),
            )
                .into_response();
        }
    };

    // Build the rkey
    let rkey = match jacquard_common::types::string::RecordKey::any(&code) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid record key: {e}")).into_response();
        }
    };

    // Build DeleteRecord request
    let request = DeleteRecord::new()
        .repo(AtIdentifier::Did(did))
        .collection(<Link as Collection>::NSID.to_string())
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
