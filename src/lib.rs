//! atpr.to — AT Protocol URL shortener, deployed on AWS Lambda.
//!
//! `missing_docs` is enforced via `[lints.rust]` in `Cargo.toml` so that local
//! builds and CI agree without either restating the lint.
extern crate alloc;

/// HTTP adapters: handlers and extractors.
pub mod api;
/// AT Protocol OAuth login, callback, and session handling.
pub mod auth;
/// Application configuration loaded from defaults, `Config.toml`, and `ATPR__` env vars.
pub mod config;
/// Short-link domain types: validated codes and destinations.
pub mod domain;
/// The application error type.
pub mod error;
#[allow(
    missing_docs,
    clippy::new_ret_no_self,
    clippy::new_without_default,
    clippy::needless_update
)]
#[rustfmt::skip]
/// Auto-generated Lexicon types.
pub mod generated;
/// The public read side: resolving `@handle/code` to a destination.
pub mod resolver;
/// OAuth session persistence.
pub mod session;
/// The authenticated write side: a user's link store.
pub mod store;

use std::sync::Arc;
use std::time::Duration;

use axum::http::{header, HeaderValue, StatusCode};
use axum::{extract::State, routing::delete, routing::get, routing::post, Router};
use jacquard::identity::resolver::ResolverOptions;
use jacquard::identity::JacquardResolver;
use tower::ServiceBuilder;
use tower_governor::errors::GovernorError;
use tower_governor::key_extractor::{KeyExtractor, SmartIpKeyExtractor};
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

/// Where the JSON API is mounted.
///
/// Named rather than spelled twice: anything that needs to know where JSON
/// territory begins — content negotiation, most obviously — has to agree with
/// the router, and two string literals that must match is exactly the pair
/// that drifts.
pub const API_PREFIX: &str = "/api";

/// Rate-limit key: the client IP when one can be determined, otherwise a single
/// shared bucket.
///
/// `SmartIpKeyExtractor` alone returns `Err` when it cannot find an IP, which
/// tower_governor renders as a 500. API Gateway does set `X-Forwarded-For`, but
/// a request arriving without it should still be *rate limited*, not rejected —
/// so falling back to one shared bucket is the safe failure mode. That is also
/// exactly the old `GlobalKeyExtractor` behaviour, now reached only when the
/// per-IP path is unavailable rather than always.
#[derive(Clone, Copy)]
struct ClientIpKeyExtractor;

impl KeyExtractor for ClientIpKeyExtractor {
    type Key = Option<std::net::IpAddr>;

    fn extract<T>(&self, req: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        Ok(SmartIpKeyExtractor.extract(req).ok())
    }
}

/// Shared application state passed to all route handlers.
///
/// Generic over the one seam that genuinely needs swapping. Production wires
/// [`auth::OAuthAuthenticator`] (`Store = PdsLinkStore`); tests wire
/// [`auth::FakeAuthenticator`] (`Store = InMemoryLinkStore`).
/// `router_with_state` monomorphises, so this costs nothing at runtime — no
/// boxing, no dynamic dispatch, no `#[async_trait]`.
///
/// The resolver needs no such treatment: tests swap it by pointing
/// `slingshot_url` at a mock server.
pub struct AppState<A: auth::Authenticator> {
    /// Turns a cookie into an authenticated user and their link store.
    pub auth: A,
    /// The public read path.
    pub links: resolver::Chained,
    /// DID → handle lookups, for building short URLs and the dashboard.
    pub identity: IdentityService,
    /// HTTP client, used directly only by the health probe.
    pub http: reqwest::Client,
    /// Loaded application configuration.
    pub config: config::Config,
}

/// Resolves a DID to its primary handle.
///
/// Kept as a service rather than a free function so the `JacquardResolver`
/// inside it is long-lived — it was rebuilt per call at two sites, so nothing
/// ever cached a DID document despite jacquard's `cache` feature being on.
pub struct IdentityService {
    http: reqwest::Client,
    identity: auth::Resolver,
    slingshot_url: String,
}

impl IdentityService {
    /// Build the service.
    pub fn new(http: reqwest::Client, identity: auth::Resolver, slingshot_url: &str) -> Self {
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

/// Build the shared outbound HTTP client.
///
/// Every outbound call in the app goes through this one client, so the timeouts
/// apply everywhere. Without them the only bound on a hung upstream is Lambda's
/// own 30s function timeout.
pub fn http_client(config: &config::Config) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(
            config.http_timeout_ms.get(),
        ))
        .connect_timeout(std::time::Duration::from_millis(
            config.http_connect_timeout_ms.get(),
        ))
        .user_agent(concat!("atpr.to/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("failed to build HTTP client")
}

/// Build the identity resolver over the shared HTTP client.
///
/// Mirrors `JacquardResolver::default()` but reuses our client instead of
/// spawning a fresh untimed one.
pub fn identity_resolver(http: reqwest::Client) -> auth::Resolver {
    JacquardResolver::new(http, ResolverOptions::default())
        .with_system_dns()
        .with_cache()
}

/// Assemble the production application state from a loaded config.
///
/// Async because opening a file-backed session store is IO. Doing that at
/// startup rather than lazily means a store we cannot read fails the cold start
/// with a clear message instead of failing every login later.
pub async fn build_state(
    config: config::Config,
) -> Result<Arc<AppState<auth::OAuthAuthenticator>>, std::io::Error> {
    let http = http_client(&config);
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
    config: config::Config,
    authenticator: A,
    http: reqwest::Client,
) -> Arc<AppState<A>> {
    Arc::new(AppState {
        auth: authenticator,
        links: resolver::Chained::new(
            resolver::Slingshot::new(http.clone(), &config.slingshot_url),
            resolver::Direct::new(http.clone(), identity_resolver(http.clone())),
        ),
        identity: IdentityService::new(
            http.clone(),
            identity_resolver(http.clone()),
            &config.slingshot_url,
        ),
        http,
        config,
    })
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
    let config = config::Config {
        slingshot_url: UNREACHABLE_SLINGSHOT.to_string(),
        ..config::Config::default()
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
/// authenticated routes without a live PDS.
pub fn router_with_state<A: auth::Authenticator>(state: Arc<AppState<A>>) -> Router {
    // 2 req/s sustained, burst of 10, keyed per client IP. API Gateway sets
    // `X-Forwarded-For`, which `SmartIpKeyExtractor` reads; a global key would
    // let one noisy client exhaust the budget for everyone on the instance.
    //
    // On Lambda this is per-execution-environment either way, so it is a
    // backstop, not the real limit — that belongs at API Gateway.
    // `finish()` returns None only for a zero rate or burst, which `NonZero*` in
    // the config types makes unrepresentable — so this cannot be the cold-start
    // panic it used to be.
    let governor_config = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(state.config.rate_limit.per_second.get())
            .burst_size(state.config.rate_limit.burst_size.get())
            .key_extractor(ClientIpKeyExtractor)
            .finish()
            .expect("rate limit values are nonzero by construction"),
    );

    // Everything that mutates state or costs an upstream round trip. `/links`
    // and `/oauth/callback` were previously unlimited.
    let rate_limited_api = Router::new()
        .route("/login", post(auth::login::<A>))
        .route("/logout", post(api::logout::logout))
        .route("/shorten", post(api::shorten::shorten::<A>))
        .route("/shorten/{code}", delete(api::delete::delete_link::<A>))
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
    // environment and nothing to read afterwards.
    let request_timeout = Duration::from_millis(state.config.request_timeout_ms.get());

    // HSTS over plain HTTP is ignored by browsers, but sending it from a
    // loopback dev server is still wrong: get the header onto `localhost` in a
    // browser that does honour it and every other local service on that host
    // becomes unreachable over http. `None` omits the header.
    let hsts = (!state.config.base_url.is_loopback())
        .then(|| HeaderValue::from_static("max-age=63072000; includeSubDomains"));

    Router::new()
        .route("/static/{*path}", get(api::static_files::static_file::<A>))
        .route("/", get(api::ui::home))
        .route("/dashboard", get(api::ui::dashboard::<A>))
        .route(
            "/oauth-client-metadata.json",
            get(auth::client_metadata::<A>),
        )
        .route("/@{handle}/{code}", get(api::resolve::resolve::<A>))
        .route("/@{handle}/{code}/info", get(api::info::info::<A>))
        .route("/@{handle}/{code}/qr", get(api::qr::qr_code::<A>))
        .merge(rate_limited_oauth)
        .nest(API_PREFIX, api_router)
        .fallback(not_found)
        // 504 rather than the layer's default 408: 408 says the *client* was
        // too slow sending its request, and that is never what happened here.
        // Every route that can run long is waiting on a PDS or a relay, which
        // is precisely what a gateway timeout describes.
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            request_timeout,
        ))
        // Outside the timeout, so its 504 gets a page too, and outside the
        // whole router, so axum's own rejections do as well.
        .layer(axum::middleware::from_fn(api::error_page::html_errors))
        .layer(
            // Outside the timeout, so the 504 it synthesises carries them too.
            // Inside, the timeout short-circuits before this layer ever sees
            // the response, and the one status most likely to be served from a
            // stranger's browser went out with no CSP, no HSTS and no nosniff.
            //
            // `if_not_present` throughout, so a handler with its own policy —
            // `Cache-Control` on redirects, say — still wins.
            ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::X_CONTENT_TYPE_OPTIONS,
                    HeaderValue::from_static("nosniff"),
                ))
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::REFERRER_POLICY,
                    HeaderValue::from_static("strict-origin-when-cross-origin"),
                ))
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::CONTENT_SECURITY_POLICY,
                    HeaderValue::from_static(CSP),
                ))
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::STRICT_TRANSPORT_SECURITY,
                    hsts,
                )),
        )
        // Outermost, so it sees the response the client actually gets —
        // including the limiter's 429s and the timeout's 504s. There was no
        // request logging at all before this.
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Content Security Policy sent on every response.
///
/// No inline script or style, and the only third-party asset is the dashboard
/// avatar — `img-src https:` is what keeps that working. Tighten it to the
/// AppView's CDN once the rebrand settles on a stack; whatever that stack is,
/// it has to be same-origin or this needs revisiting, which is the point of
/// setting the header before the rebrand rather than after.
///
/// `form-action` allows `https:` because sign-in depends on it. `POST /login`
/// answers a browser form with a 303 to the user's *own* PDS authorize URL,
/// and Chromium enforces `form-action` against the redirect target, not just
/// the form's action — so `'self'` alone blocks the navigation and the login
/// button appears to do nothing. The PDS origin is per-user and unknowable
/// here, so an exact allowlist is not available. `https:` still refuses
/// `javascript:` and `data:` form targets, which is the attack this directive
/// is actually for.
const CSP: &str = "default-src 'self'; \
     img-src 'self' https: data:; \
     style-src 'self'; \
     script-src 'self'; \
     form-action 'self' https:; \
     frame-ancestors 'none'; \
     base-uri 'none'";

/// Catch-all for unmatched paths, so `/favicon.ico` and friends get a body
/// rather than axum's default empty 404.
async fn not_found() -> error::AppError {
    error::AppError::NotFound
}

/// Liveness probe.
///
/// Pings Slingshot's root rather than resolving a specific handle: the old probe
/// hardcoded `atpr.to`, so it reported the service degraded whenever that one
/// identity was unresolvable, regardless of Slingshot's actual health.
///
/// Returns 503 when degraded. It previously answered 200 while reporting
/// `"status":"degraded"` in the body, so uptime checks keying on the status code
/// — which is most of them — never fired.
async fn health<A: auth::Authenticator>(
    State(state): State<Arc<AppState<A>>>,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let ping_url = format!("{}/", state.config.slingshot_url.trim_end_matches('/'));

    let slingshot_ok = match state.http.get(&ping_url).send().await {
        Ok(r) if r.status().is_success() => true,
        Ok(r) => {
            tracing::warn!(status = %r.status(), url = %ping_url, "slingshot health probe failed");
            false
        }
        Err(e) => {
            tracing::warn!(err = %e, url = %ping_url, "slingshot health probe failed");
            false
        }
    };

    let status = if slingshot_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        axum::Json(serde_json::json!({
            "status": if slingshot_ok { "ok" } else { "degraded" },
            "slingshot": if slingshot_ok { "ok" } else { "unreachable" },
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    /// The policy is a constant, so what it permits is worth asserting
    /// directly rather than only through whatever a handler happens to do.
    ///
    /// The `unsafe-*` assertions are the load-bearing ones: the client side is
    /// vanilla JS with every stylesheet and script same-origin, so nothing
    /// here needs to evaluate a string or parse an inline style. Both are easy
    /// to add under deadline and effectively impossible to remove afterwards,
    /// so a future loosening should fail a test that says why.
    #[test]
    fn test_csp_permits_what_the_app_needs_and_nothing_looser() {
        assert!(CSP.contains("default-src 'self'"));

        // Sign-in 303s to the user's own PDS, and Chromium checks
        // `form-action` against redirect targets.
        assert!(
            CSP.contains("form-action 'self' https:"),
            "cross-origin sign-in redirect must be permitted"
        );

        assert!(!CSP.contains("unsafe-eval"), "the client JS is vanilla");
        assert!(
            !CSP.contains("unsafe-inline"),
            "all CSS and JS is same-origin and external"
        );
    }

    async fn test_state_with_slingshot(
        slingshot_url: String,
    ) -> std::sync::Arc<AppState<auth::OAuthAuthenticator>> {
        let cfg = config::Config {
            slingshot_url,
            ..config::Config::default()
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

    // `test_health_route` lived here. It probed the real Slingshot and, because
    // the status now reflects genuine upstream health, could only assert
    // "not 404, not 405" — which `test_health_ok` and `test_health_degraded`
    // already establish hermetically, along with the status codes.

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
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
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
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // The status code must carry the failure: uptime checks key on it, and
        // this endpoint used to answer 200 while the body said "degraded".
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "degraded");
        assert_eq!(json["slingshot"], "unreachable");
    }

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
