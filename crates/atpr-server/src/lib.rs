//! atpr.to — AT Protocol URL shortener, deployed on AWS Lambda.
//!
//! The authenticated application around the shared read path in `atpr-core`:
//! OAuth sign-in, the PDS-backed link store, the dashboard, and the Lambda
//! entry point. The public redirect route is served by
//! [`atpr_core::redirect::resolve`] operating on the nested
//! [`atpr_core::redirect::ResolveState`] — see the `FromRef` impl below.
//!
//! `missing_docs` is enforced via `[lints.rust]` in `Cargo.toml` so that local
//! builds and CI agree without either restating the lint.
#![recursion_limit = "256"]
// Same reason as atpr-core: the PDS store's AFIT futures (putRecord with its
// swap-record precondition, listRecords paging) overflow nightly's default
// trait-solving depth while proving `Send`.
extern crate alloc;

/// HTTP adapters: handlers and extractors.
pub mod api;
/// AT Protocol OAuth login, callback, and session handling.
pub mod auth;
/// Auto-generated Lexicon types.
#[allow(
    missing_docs,
    clippy::new_ret_no_self,
    clippy::new_without_default,
    clippy::needless_update
)]
#[rustfmt::skip]
pub mod generated;
/// OAuth session persistence.
pub mod session;
/// The authenticated write side: a user's link store.
pub mod store;

// The read path lives in `atpr-core`; these re-exports keep `crate::domain`,
// `crate::error`, `crate::config` and `crate::resolver` paths working for the
// modules moved here verbatim, and give handlers one import site.
pub use atpr_core::{config, domain, error, resolver};

use std::sync::Arc;

use axum::extract::State;
use axum::{routing::delete, routing::get, routing::post, Router};
use tower_governor::GovernorLayer;
use tower_http::trace::TraceLayer;

use atpr_core::config::Config;
use atpr_core::redirect::{self, ResolveState};
use atpr_core::{error_page, identity};

/// Build the production application state from a loaded config.
///
/// Async because opening a file-backed session store is IO. Doing that at
/// startup rather than lazily means a store we cannot read fails the cold start
/// with a clear message instead of failing every login later.
pub async fn build_state(
    config: Config,
) -> Result<Arc<AppState<auth::OAuthAuthenticator>>, std::io::Error> {
    let http = identity::http_client(&config);
    let oauth =
        auth::build_oauth_client(&config.base_url, &config.session_store, http.clone()).await?;
    Ok(build_state_with(
        config,
        auth::OAuthAuthenticator::new(oauth),
        http,
    ))
}

/// Assemble application state around a caller-supplied authenticator.
///
/// The composition root, parameterised. Production passes
/// [`auth::OAuthAuthenticator`]; tests pass [`auth::FakeAuthenticator`] and an
/// HTTP client pointed at a mock server.
pub fn build_state_with<A: auth::Authenticator>(
    config: Config,
    authenticator: A,
    http: reqwest::Client,
) -> Arc<AppState<A>> {
    Arc::new(AppState {
        auth: authenticator,
        redirect: Arc::new(ResolveState::new(config.clone(), http.clone())),
        identity: IdentityService::new(
            http.clone(),
            identity::identity_resolver(http.clone()),
            &config.slingshot_url,
        ),
        http,
        config,
    })
}

/// Shared application state passed to all route handlers.
///
/// Generic over the one seam that genuinely needs swapping. Production wires
/// [`auth::OAuthAuthenticator`] (`Store = PdsLinkStore`); tests wire
/// [`auth::FakeAuthenticator`] (`Store = InMemoryLinkStore`).
/// `router_with_state` monomorphises, so this costs nothing at runtime — no
/// boxing, no dynamic dispatch, no `#[async_trait]`.
///
/// The resolver needs no such treatment: it is carried inside `redirect` and
/// swapped in tests by pointing `slingshot_url` at a mock server.
pub struct AppState<A: auth::Authenticator> {
    /// Turns a cookie into an authenticated user and their link store.
    pub auth: A,
    /// The public read path, shared verbatim with `atpr-redirect`.
    pub redirect: Arc<ResolveState>,
    /// DID → handle lookups, for building short URLs and the dashboard.
    pub identity: IdentityService,
    /// HTTP client, used directly only by the health probe.
    pub http: reqwest::Client,
    /// Loaded application configuration.
    pub config: Config,
}

// Thin adapters from `AppState` to the shared read-path handlers, which
// operate on the nested [`ResolveState`] so the standalone server can serve
// them directly. The orphan rules forbid a generic `FromRef` impl for a
// foreign `Arc`, so the delegation is spelled out — one line of glue per
// route, and every handler body still lives in exactly one place.

/// `GET /@{handle}/{code}` — see [`redirect::resolve`].
pub async fn resolve<A: auth::Authenticator>(
    State(state): State<Arc<AppState<A>>>,
    path: axum::extract::Path<(String, String)>,
) -> Result<axum::response::Response, error::AppError> {
    redirect::resolve(State(Arc::clone(&state.redirect)), path).await
}

/// `GET /api/health` — see [`redirect::health`].
pub async fn health<A: auth::Authenticator>(
    State(state): State<Arc<AppState<A>>>,
) -> axum::response::Response {
    redirect::health(State(Arc::clone(&state.redirect))).await
}

/// `GET /@{handle}/{code}/info` — see [`api::info::info`].
pub async fn info<A: auth::Authenticator>(
    State(state): State<Arc<AppState<A>>>,
    path: axum::extract::Path<(String, String)>,
) -> Result<axum::response::Response, error::AppError> {
    api::info::info(State(Arc::clone(&state.redirect)), path).await
}

/// `GET /@{handle}/{code}/qr` — see [`api::qr::qr_code`].
pub async fn qr<A: auth::Authenticator>(
    State(state): State<Arc<AppState<A>>>,
    path: axum::extract::Path<(String, String)>,
) -> Result<axum::response::Response, error::AppError> {
    api::qr::qr_code(State(Arc::clone(&state.redirect)), path).await
}

/// Resolves a DID to its primary handle.
///
/// Kept as a service rather than a free function so the resolver
/// inside it is long-lived — it was rebuilt per call at two sites, so nothing
/// ever cached a DID document despite jacquard's `cache` feature being on.
pub struct IdentityService {
    http: reqwest::Client,
    identity: identity::Resolver,
    slingshot_url: String,
}

impl IdentityService {
    /// Build the service.
    pub fn new(http: reqwest::Client, identity: identity::Resolver, slingshot_url: &str) -> Self {
        Self {
            http,
            identity,
            slingshot_url: slingshot_url.trim_end_matches('/').to_string(),
        }
    }

    /// Resolve a DID to its primary handle.
    ///
    /// Tries Slingshot's `describeRepo` (1 hop), then the DID document directly.
    /// `None` means neither worked.
    pub async fn handle_for(&self, did_str: &str) -> Option<String> {
        let url = format!(
            "{}/xrpc/com.atproto.repo.describeRepo?repo={}",
            self.slingshot_url,
            urlencoding::encode(did_str),
        );
        if let Ok(resp) = self.http.get(&url).send().await {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(handle) = body.get("handle").and_then(|h| h.as_str()) {
                        return Some(handle.to_string());
                    }
                }
            }
        }

        use jacquard::identity::resolver::IdentityResolver;
        use jacquard_common::types::did::Did;
        let did: Did = Did::new_owned(did_str).ok()?;
        let doc_response = self.identity.resolve_did_doc(&did).await.ok()?;
        let doc = doc_response.parse().ok()?;
        doc.handles()
            .into_iter()
            .next()
            .map(|h| h.as_ref().to_string())
    }
}

/// Build the application router, loading config from the environment.
pub async fn router() -> Router {
    let state = build_state(config::load())
        .await
        .unwrap_or_else(|e| panic!("failed to open the session store: {e}"));
    router_with_state(state)
}

/// A Slingshot URL that is guaranteed not to answer.
///
/// Port 1 on loopback: connection refused immediately, no DNS, no packets off
/// the machine.
#[cfg(test)]
pub(crate) const UNREACHABLE_SLINGSHOT: &str = "http://127.0.0.1:1";

/// Build a router for tests, from compiled defaults.
///
/// Two hermeticity properties, both of which the suite lacked:
///
/// - It ignores `Config.toml`, which points `session_file` at a real file — so
///   every test that called `router()` shared one on-disk session store and
///   raced the others.
/// - It points Slingshot at an address that cannot answer, so a test cannot
///   silently start depending on `slingshot.microcosm.blue` being up. Tests that
///   need an upstream mount a mock and say so.
#[cfg(test)]
pub(crate) async fn test_router() -> Router {
    let config = Config {
        slingshot_url: UNREACHABLE_SLINGSHOT.to_string(),
        ..Config::default()
    };
    router_with_state(
        build_state(config)
            .await
            .expect("the in-memory store cannot fail"),
    )
}

/// Build the application router from an existing `AppState`.
///
/// Generic over the authenticator, so tests can wire a fake one and drive the
/// authenticated routes without a live PDS. Middleware comes from `atpr-core`
/// helpers so both routers stay byte-identical where they overlap.
pub fn router_with_state<A: auth::Authenticator>(state: Arc<AppState<A>>) -> Router {
    // 2 req/s sustained, burst of 10, keyed per client IP, guarding everything
    // that mutates state or costs an upstream round trip. `/links` and
    // `/oauth/callback` were previously unlimited.
    let governor_config = redirect::client_ip_rate_limit(&state.config);

    // Everything that mutates state or costs an upstream round trip.
    let rate_limited_api = Router::new()
        .route("/login", post(auth::login::<A>))
        .route("/logout", post(api::logout::logout))
        .route("/shorten", post(api::shorten::shorten::<A>))
        .route(
            "/shorten/{code}",
            delete(api::delete::delete_link::<A>).put(api::update::update_link::<A>),
        )
        .route("/links", get(api::links::list_links::<A>))
        .layer(GovernorLayer::new(governor_config.clone()));

    let api_router = Router::new()
        .route("/health", get(health::<A>))
        .merge(rate_limited_api);

    let rate_limited_oauth = Router::new()
        .route("/oauth/callback", get(auth::oauth_callback::<A>))
        .layer(GovernorLayer::new(governor_config));

    // Below the Lambda function timeout on purpose: overrunning here produces a
    // 408 and a log line, where overrunning there produces a killed execution
    // environment and nothing to read afterwards. (The 504 mapping itself lives
    // in `redirect::timeout_layer`.)
    Router::new()
        .route("/static/{*path}", get(api::static_files::static_file::<A>))
        .route("/", get(api::ui::home))
        .route("/dashboard", get(api::ui::dashboard::<A>))
        .route(
            "/oauth-client-metadata.json",
            get(auth::client_metadata::<A>),
        )
        .route("/@{handle}/{code}", get(resolve::<A>))
        .route("/@{handle}/{code}/info", get(info::<A>))
        .route("/@{handle}/{code}/qr", get(qr::<A>))
        .merge(rate_limited_oauth)
        .nest(atpr_core::API_PREFIX, api_router)
        .fallback(not_found)
        .layer(redirect::timeout_layer(&state.config))
        // Outside the timeout, so its 504 gets a page too, and outside the
        // whole router, so axum's own rejections do as well.
        .layer(axum::middleware::from_fn(error_page::html_errors))
        .layer(redirect::security_headers(redirect::hsts_header(
            &state.config,
        )))
        // Outermost, so it sees the response the client actually gets —
        // including the limiter's 429s and the timeout's 504s.
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Catch-all for unmatched paths, so `/favicon.ico` and friends get a body
/// rather than axum's default empty 404.
async fn not_found() -> error::AppError {
    error::AppError::NotFound
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn test_state_with_slingshot(
        slingshot_url: String,
    ) -> std::sync::Arc<AppState<auth::OAuthAuthenticator>> {
        let cfg = Config {
            slingshot_url,
            ..Config::default()
        };
        build_state(cfg).await.expect("in-memory store cannot fail")
    }

    #[tokio::test]
    async fn test_index_route() {
        let app = test_router().await;
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_shorten_requires_post() {
        let app = test_router().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/shorten")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    /// Route existence only. Backed by a mock that answers 404 authoritatively,
    /// so no fallback runs — the direct resolver would otherwise do real DNS on
    /// `alice.bsky.social`, which is how this test used to reach the network.
    #[tokio::test]
    async fn test_resolve_route_exists() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock)
            .await;

        let app = router_with_state(test_state_with_slingshot(mock.uri()).await);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/@alice.bsky.social/abc123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_oauth_metadata_route() {
        let app = test_router().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/oauth-client-metadata.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_delete_requires_auth() {
        let app = test_router().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/shorten/abc123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_method() {
        let app = test_router().await;
        // GET on a DELETE-only route should be 405
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/shorten/abc123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    // Health-probe tests moved with the handler into `atpr-core::redirect`;
    // the status-code contract is pinned there hermetically.

    #[tokio::test]
    async fn test_auth_session_invalid_did() {
        let app = test_router().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/shorten/abc123")
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
        let app = test_router().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/shorten/abc123")
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
        let app = test_router().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}
