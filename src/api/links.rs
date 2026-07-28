//! `GET /api/links` — list the caller's short links.

use axum::extract::Query;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::auth::{AuthedUser, Authenticator};
use crate::error::AppError;
use crate::store::{LinkStore, PageRequest, MAX_PAGE_SIZE};

/// Query parameters for `GET /api/links`.
#[derive(Debug, Deserialize, Default)]
pub struct ListQuery {
    /// Page size. Clamped to `1..=100`.
    pub limit: Option<u8>,
    /// Cursor from a previous response.
    pub cursor: Option<String>,
}

/// List the authenticated user's shortened links.
///
/// Returns `{ links: [{ code, url, updated_at }], cursor }`. The cursor used to
/// be discarded and no `limit` was sent at all, so anyone with more links than
/// the server's default page size silently lost the rest.
#[tracing::instrument(skip_all)]
pub async fn list_links<A: Authenticator>(
    user: AuthedUser<A>,
    Query(query): Query<ListQuery>,
) -> Result<Response, AppError> {
    let page = user
        .store
        .list(PageRequest {
            limit: query.limit.unwrap_or(MAX_PAGE_SIZE),
            cursor: query.cursor,
        })
        .await
        .map_err(AppError::from)?;

    let links: Vec<serde_json::Value> = page
        .links
        .iter()
        .map(|entry| {
            serde_json::json!({
                "code": entry.code.as_str(),
                "url": entry.link.target.as_str(),
                "updated_at": entry.link.updated_at.as_deref().unwrap_or(""),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "links": links,
        "cursor": page.cursor,
    }))
    .into_response())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::router;

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
