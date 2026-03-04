pub mod auth;
pub mod error;
pub mod generated;
pub mod resolve;
pub mod shorten;

use std::sync::Arc;

use axum::{routing::get, routing::post, Router};

pub struct AppState {
    pub oauth: auth::OAuthClientType,
}

pub fn router() -> Router {
    let state = Arc::new(AppState {
        oauth: auth::build_oauth_client(),
    });

    Router::new()
        .route("/", get(index))
        .route(
            "/.well-known/oauth-client-metadata.json",
            get(auth::client_metadata),
        )
        .route("/oauth/callback", get(auth::oauth_callback))
        .route("/login", post(auth::login))
        .route("/shorten", post(shorten::shorten))
        .route("/@{handle}/{code}", get(resolve::resolve))
        .with_state(state)
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

        // Route exists — should not be 405 (Method Not Allowed).
        // Will be 404 or 502 since resolution hits network.
        assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
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

    #[tokio::test]
    async fn test_login_requires_post() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}
