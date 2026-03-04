pub mod auth;
pub mod generated;
pub mod resolve;
pub mod shorten;

use axum::{routing::get, routing::post, Router};

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route(
            "/.well-known/oauth-client-metadata.json",
            get(auth::client_metadata),
        )
        .route("/oauth/callback", get(auth::oauth_callback))
        .route("/shorten", post(shorten::shorten))
        .route("/@{handle}/{code}", get(resolve::resolve))
}

async fn index() -> &'static str {
    "atpr.to \u{2014} AT Protocol URL Shortener"
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn test_index_route() {
        let app = router();
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_shorten_requires_post() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/shorten")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_resolve_route_exists() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/@alice.bsky.social/abc123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should not be 404 (route exists), will be 502 or similar since resolution isn't wired yet
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_oauth_metadata_route() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/.well-known/oauth-client-metadata.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
