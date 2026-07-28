//! `DELETE /api/shorten/{code}` — remove a short link.

use axum::extract::Path;
use axum::http::StatusCode;

use crate::auth::{AuthedUser, Authenticator};
use crate::domain::ShortCode;
use crate::error::AppError;
use crate::store::LinkStore;

/// Delete a short URL record. Requires authentication.
#[tracing::instrument(skip_all, fields(code))]
pub async fn delete_link<A: Authenticator>(
    user: AuthedUser<A>,
    Path(code): Path<String>,
) -> Result<StatusCode, AppError> {
    let code = ShortCode::parse(&code)?;
    // `skip_all` was paired with `fields(code)` and nothing ever recorded it,
    // so deletions were logged without saying what was deleted.
    tracing::Span::current().record("code", code.as_str());

    user.store.delete(&code).await.map_err(AppError::from)?;
    Ok(StatusCode::NO_CONTENT)
}
