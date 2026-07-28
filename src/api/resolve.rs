//! `GET /@{handle}/{code}` — resolve a short link and redirect.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use jacquard_common::types::string::Handle;

use crate::auth::Authenticator;
use crate::domain::ShortCode;
use crate::error::AppError;
use crate::resolver::LinkResolver;
use crate::AppState;

/// Resolve a short URL and redirect.
///
/// All the strategy — relay first, PDS second, when to fall back — lives in
/// `resolver::Chained`. This is only the HTTP edge: parse the path segments,
/// hand them to the resolver, turn the answer into a response.
#[tracing::instrument(skip(state), fields(handle = %handle, code = %code))]
pub async fn resolve<A: Authenticator>(
    State(state): State<Arc<AppState<A>>>,
    Path((handle, code)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let parsed_handle: Handle =
        Handle::new_owned(&handle).map_err(|_| AppError::BadRequest("Invalid handle"))?;
    let parsed_code = ShortCode::parse(&code)?;

    let link = state.links.resolve(&parsed_handle, &parsed_code).await?;

    // `link.target` is a `TargetUrl`, so the scheme has already been checked.
    // There is no raw string here that could carry `javascript:`.
    Ok(Redirect::temporary(link.target.as_str()).into_response())
}

#[cfg(test)]
mod tests {
    use jacquard_common::deps::smol_str::SmolStr;
    use jacquard_common::types::string::Handle;

    #[test]
    fn test_handle_parsing() {
        assert!(Handle::<SmolStr>::new_owned("alice.bsky.social").is_ok());
        assert!(Handle::<SmolStr>::new_owned("seqre.dev").is_ok());
        // Single-label handles are invalid per AT Protocol.
        assert!(Handle::<SmolStr>::new_owned("invalid").is_err());
    }
}
