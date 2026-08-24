//! `PUT /api/shorten/{code}` — point an existing short link somewhere else.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::auth::{AuthedUser, Authenticator};
use crate::domain::{ShortCode, TargetUrl};
use crate::error::AppError;
use crate::store::{LinkStore, PutMode};

/// Request body for `PUT /api/shorten/{code}`.
#[derive(Deserialize)]
pub struct UpdateRequest {
    /// The new destination URL.
    pub url: String,
}

/// Repoint a short link. Requires authentication.
///
/// The store has been able to do this since it was written — [`PutMode`] has
/// both variants and both are tested — but nothing exposed it, so a
/// destination was fixed for the life of the record and "editing" one meant
/// deleting it and losing the code in between.
///
/// `POST` refusing to overwrite and `PUT` agreeing to is the intended
/// difference, not an inconsistency. The 409 on `POST` exists to stop a
/// duplicate code quietly destroying a link somebody still has; arriving here
/// is a statement that replacing it is the point. A write can only ever reach
/// the caller's own repository, so the worst case is replacing something you
/// own on purpose.
///
/// Upsert rather than strict edit: a code with no record yet is created. The
/// alternative is a read before every write, which costs a round trip and is
/// still a race.
///
/// Returns 204 rather than the short URL. Nothing about the address changes
/// here, and `POST` only returns one because it has to resolve the handle to
/// build it — a hop that can fail *after* the record is safely written.
#[tracing::instrument(skip_all, fields(code))]
pub async fn update_link<A: Authenticator>(
    user: AuthedUser<A>,
    Path(code): Path<String>,
    Json(body): Json<UpdateRequest>,
) -> Result<StatusCode, AppError> {
    let code = ShortCode::parse(&code)?;
    let target = TargetUrl::parse(&body.url)?;
    tracing::Span::current().record("code", code.as_str());

    user.store
        .put(&code, &target, PutMode::Overwrite)
        .await
        .map_err(AppError::from)?;

    Ok(StatusCode::NO_CONTENT)
}
