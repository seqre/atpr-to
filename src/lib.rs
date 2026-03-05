//! atpr.to — AT Protocol URL shortener, deployed on AWS Lambda.
#![warn(missing_docs)]

/// AT Protocol OAuth login, callback, and session handling.
pub mod auth;
/// Application configuration loaded from defaults, `Config.toml`, and `ATPR__` env vars.
pub mod config;
/// Short URL deletion endpoint.
pub mod delete;
/// Authenticated link listing endpoint.
pub mod links;
/// Session logout endpoint.
pub mod logout;
/// HTML error page helpers.
pub mod error;
#[allow(missing_docs, clippy::new_ret_no_self, clippy::new_without_default)]
/// Auto-generated Lexicon types.
pub mod generated;
/// QR code generation for short URLs.
pub mod qr;
/// Short URL resolution and redirect.
pub mod resolve;
/// Short URL creation endpoint.
pub mod shorten;
/// HTML UI (home page and dashboard).
pub mod ui;

use std::sync::Arc;

use axum::{extract::State, routing::delete, routing::get, routing::post, Router};
use tower_governor::{governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor, GovernorLayer};

/// Shared application state passed to all route handlers.
pub struct AppState {
    /// AT Protocol OAuth client for login and session management.
    pub oauth: auth::OAuthClientType,
    /// HTTP client for outbound requests (Slingshot relay, DID resolution).
    pub http: reqwest::Client,
    /// Loaded application configuration.
    pub config: config::Config,
}

/// Build the application router, loading config from the environment.
pub fn router() -> Router {
    let config = config::load();

    let state = Arc::new(AppState {
        oauth: auth::build_oauth_client(&config.base_url),
        http: reqwest::Client::new(),
        config,
    });

    router_with_state(state)
}

/// Build the application router from an existing `AppState`.
///
/// Useful for testing or when state is constructed externally.
pub fn router_with_state(state: Arc<AppState>) -> Router {

    // Rate limit mutation routes: 2 req/s sustained, burst of 10.
    // On Lambda, state is per-instance; use API Gateway throttling for global limits.
    let governor_config = GovernorConfigBuilder::default()
        .per_second(state.config.rate_limit.per_second)
        .burst_size(state.config.rate_limit.burst_size)
        .key_extractor(GlobalKeyExtractor)
        .finish()
        .unwrap();

    let rate_limited = Router::new()
        .route("/login", post(auth::login))
        .route("/logout", post(logout::logout))
        .route("/shorten", post(shorten::shorten))
        .route("/shorten/{code}", delete(delete::delete_link))
        .layer(GovernorLayer::new(governor_config));

    Router::new()
        .route("/", get(ui::home))
        .route("/dashboard", get(ui::dashboard))
        .route("/links", get(links::list_links))
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

async fn health(State(state): State<Arc<AppState>>) -> axum::Json<serde_json::Value> {
    let ping_url = format!(
        "{}/xrpc/com.atproto.identity.resolveHandle?handle=atpr.to",
        state.config.slingshot_url.trim_end_matches('/'),
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
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn test_state_with_slingshot(slingshot_url: String) -> std::sync::Arc<AppState> {
        let cfg = config::Config {
            slingshot_url,
            ..config::Config::default()
        };
        std::sync::Arc::new(AppState {
            oauth: auth::build_oauth_client(&cfg.base_url),
            http: reqwest::Client::new(),
            config: cfg,
        })
    }

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
    async fn test_health_ok() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "did": "did:plc:test" })),
            )
            .mount(&mock)
            .await;

        let state = test_state_with_slingshot(mock.uri()).await;
        let app = router_with_state(state);
        let response = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["slingshot"], "ok");
    }

    #[tokio::test]
    async fn test_health_degraded() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock)
            .await;

        let state = test_state_with_slingshot(mock.uri()).await;
        let app = router_with_state(state);
        let response = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "degraded");
        assert_eq!(json["slingshot"], "unreachable");
    }

    #[tokio::test]
    async fn test_auth_session_invalid_did() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/shorten/abc123")
                    .header("cookie", "session=notadid|session123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_session_expired() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/shorten/abc123")
                    .header("cookie", "session=did:web:example.com|nonexistent_session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
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
