pub mod auth;
pub mod delete;
pub mod error;
pub mod generated;
pub mod qr;
pub mod resolve;
pub mod shorten;

use std::sync::Arc;

use axum::{extract::State, routing::delete, routing::get, routing::post, Router};
use tower_governor::{governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor, GovernorLayer};

pub struct AppState {
    pub oauth: auth::OAuthClientType,
    pub http: reqwest::Client,
    pub slingshot_url: String,
}

pub fn router() -> Router {
    let slingshot_url = std::env::var("SLINGSHOT_URL")
        .unwrap_or_else(|_| "https://slingshot.microcosm.blue/".to_string());

    let state = Arc::new(AppState {
        oauth: auth::build_oauth_client(),
        http: reqwest::Client::new(),
        slingshot_url,
    });

    router_with_state(state)
}

pub fn router_with_state(state: Arc<AppState>) -> Router {

    // Rate limit mutation routes: 2 req/s sustained, burst of 10.
    // On Lambda, state is per-instance; use API Gateway throttling for global limits.
    let governor_config = GovernorConfigBuilder::default()
        .per_second(2)
        .burst_size(10)
        .key_extractor(GlobalKeyExtractor)
        .finish()
        .unwrap();

    let rate_limited = Router::new()
        .route("/login", post(auth::login))
        .route("/shorten", post(shorten::shorten))
        .route("/shorten/{code}", delete(delete::delete_link))
        .layer(GovernorLayer::new(governor_config));

    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route(
            "/.well-known/oauth-client-metadata.json",
            get(auth::client_metadata),
        )
        .route("/oauth/callback", get(auth::oauth_callback))
        .merge(rate_limited)
        .route("/@{handle}/{code}", get(resolve::resolve))
        .route("/@{handle}/{code}/qr", get(qr::qr_code))
        .with_state(state)
}

async fn index() -> &'static str {
    "atpr.to \u{2014} AT Protocol URL Shortener"
}

async fn health(State(state): State<Arc<AppState>>) -> axum::Json<serde_json::Value> {
    let ping_url = format!(
        "{}/xrpc/com.atproto.identity.resolveHandle?handle=atpr.to",
        state.slingshot_url.trim_end_matches('/'),
    );

    let slingshot_status = match state.http.get(&ping_url).send().await {
        Ok(r) if r.status().is_success() => "ok",
        _ => "unreachable",
    };

    let overall = if slingshot_status == "ok" { "ok" } else { "degraded" };

    axum::Json(serde_json::json!({
        "status": overall,
        "slingshot": slingshot_status,
    }))
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
    async fn test_delete_requires_auth() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/shorten/abc123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_method() {
        let app = router();
        // GET on a DELETE-only route should be 405
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/shorten/abc123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_health_route() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
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
