//! The standalone public router: `GET /@{handle}/{code}`, health, and the
//! middleware both servers share.
//!
//! [`ResolveState`] holds everything resolution needs — resolver chain, HTTP
//! client, config — and nothing it does not. That is what makes a redirect-only
//! deployment possible: `atpr-redirect` serves [`router_with_state`] directly,
//! with no authenticator in sight, while `atpr-server` nests this state inside
//! its own via a `FromRef` impl and mounts the very same handler.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::header::CACHE_CONTROL;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{middleware, Router};
use jacquard_common::types::string::Handle;
use tower_governor::errors::GovernorError;
use tower_governor::key_extractor::{KeyExtractor, SmartIpKeyExtractor};
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::domain::ShortCode;
use crate::error::AppError;
use crate::resolver::{Chained, Direct, LinkResolver, Slingshot};
use crate::{error_page, identity};

/// Everything the public read path needs.
///
/// No authenticator: resolving a link never touches one. The server binary
/// wraps this inside `AppState`; the standalone binary serves it as-is.
pub struct ResolveState {
    /// The public read path.
    pub links: Chained,
    /// HTTP client, used directly only by the health probe.
    pub http: reqwest::Client,
    /// Loaded application configuration.
    pub config: Config,
}

impl ResolveState {
    /// Build the state from a config and the shared outbound client.
    ///
    /// The same composition `atpr-server` uses, so both binaries resolve
    /// through an identical `Chained`.
    pub fn new(config: Config, http: reqwest::Client) -> Self {
        Self {
            links: Chained::new(
                Slingshot::new(http.clone(), &config.slingshot_url),
                Direct::new(http.clone(), identity::identity_resolver(http.clone())),
            ),
            http,
            config,
        }
    }
}

/// Resolve a short URL and redirect.
///
/// All the strategy — relay first, PDS second, when to fall back — lives in
/// `resolver::Chained`. This is only the HTTP edge: parse the path segments,
/// hand them to the resolver, turn the answer into a response.
#[tracing::instrument(skip(state), fields(handle = %handle, code = %code))]
pub async fn resolve(
    State(state): State<Arc<ResolveState>>,
    Path((handle, code)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let parsed_handle: Handle =
        Handle::new_owned(&handle).map_err(|_| AppError::BadRequest("Invalid handle"))?;
    let parsed_code = ShortCode::parse(&code)?;

    let link = state.links.resolve(&parsed_handle, &parsed_code).await?;

    // `link.target` is a `TargetUrl`, so the scheme has already been checked.
    // There is no raw string here that could carry `javascript:`.
    let mut response = Redirect::temporary(link.target.as_str()).into_response();

    // The redirect is the product and costs two upstream hops, with nothing
    // cached anywhere. A short shared max-age lets API Gateway, CDNs and the
    // browser answer repeat traffic without touching a PDS.
    //
    // Still a *temporary* redirect: a 301 would be cached indefinitely and
    // outlive both edits and deletes, which is the whole reason the TTL is a
    // config knob rather than a constant.
    if let Some(header) = cache_control(state.config.redirect_cache_max_age) {
        response.headers_mut().insert(CACHE_CONTROL, header);
    }
    Ok(response)
}

/// The `Cache-Control` header for a successful redirect, or `None` when caching
/// is disabled.
fn cache_control(max_age: u32) -> Option<HeaderValue> {
    if max_age == 0 {
        return None;
    }
    HeaderValue::from_str(&format!("public, max-age={max_age}")).ok()
}

/// Rate-limit key: the client IP when one can be determined, otherwise a single
/// shared bucket.
///
/// `SmartIpKeyExtractor` alone returns `Err` when it cannot find an IP, which
/// tower_governor renders as a 500. API Gateway does set `X-Forwarded-For`, but
/// a request arriving without it should still be *rate limited*, not rejected —
/// so falling back to one shared bucket is the safe failure mode. Behind a
/// plain reverse proxy, make sure the proxy overwrites (not appends to)
/// `X-Forwarded-For`, or a spoofed header lets callers dodge the per-IP limit.
#[derive(Clone, Copy)]
pub struct ClientIpKeyExtractor;

impl KeyExtractor for ClientIpKeyExtractor {
    type Key = Option<std::net::IpAddr>;

    fn extract<T>(&self, req: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        Ok(SmartIpKeyExtractor.extract(req).ok())
    }
}

/// The per-client-IP rate limiter, built from the configured rate and burst.
///
/// On Lambda this is per-execution-environment either way, so it is a backstop,
/// not the real limit — that belongs at API Gateway. For a self-hosted instance
/// with no gateway in front, it *is* the only limit, which is why the public
/// router applies it unconditionally.
///
/// `finish()` returns None only for a zero rate or burst, which `NonZero*` in
/// the config types makes unrepresentable — so this cannot be the cold-start
/// panic it used to be.
pub fn client_ip_rate_limit(
    config: &Config,
) -> Arc<
    tower_governor::governor::GovernorConfig<
        ClientIpKeyExtractor,
        governor::middleware::NoOpMiddleware,
    >,
> {
    Arc::new(
        GovernorConfigBuilder::default()
            .per_second(config.rate_limit.per_second.get())
            .burst_size(config.rate_limit.burst_size.get())
            .key_extractor(ClientIpKeyExtractor)
            .finish()
            .expect("rate limit values are nonzero by construction"),
    )
}

/// HSTS over plain HTTP is ignored by browsers, but sending it from a loopback
/// dev server is still wrong: get the header onto `localhost` in a browser that
/// does honour it and every other local service on that host becomes
/// unreachable over http. `None` omits the header.
pub fn hsts_header(config: &Config) -> Option<HeaderValue> {
    (!config.base_url.is_loopback())
        .then(|| HeaderValue::from_static("max-age=63072000; includeSubDomains"))
}

/// The security-header stack: nosniff, referrer policy, CSP, HSTS.
///
/// A tuple of concrete layers rather than something boxed or generic, so both
/// routers apply exactly these headers without restating any of them.
/// `if_not_present` throughout, so a handler with its own policy —
/// `Cache-Control` on redirects, say — still wins.
pub fn security_headers(
    hsts: Option<HeaderValue>,
) -> (
    SetResponseHeaderLayer<HeaderValue>,
    SetResponseHeaderLayer<HeaderValue>,
    SetResponseHeaderLayer<HeaderValue>,
    // HSTS is carried as `Option<HeaderValue>` so a loopback dev server can
    // omit it entirely; tower-http turns `None` into "header not set".
    SetResponseHeaderLayer<Option<HeaderValue>>,
) {
    (
        SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ),
        SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ),
        SetResponseHeaderLayer::if_not_present(header::CONTENT_SECURITY_POLICY, {
            HeaderValue::from_static(crate::CSP)
        }),
        SetResponseHeaderLayer::if_not_present(header::STRICT_TRANSPORT_SECURITY, hsts),
    )
}

/// The inbound request budget as a tower layer, answering 504 rather than the
/// default 408: 408 says the *client* was too slow sending its request, and
/// that is never what happened here. Every route that can run long is waiting
/// on a PDS or a relay, which is precisely what a gateway timeout describes.
pub fn timeout_layer(config: &Config) -> TimeoutLayer {
    TimeoutLayer::with_status_code(
        StatusCode::GATEWAY_TIMEOUT,
        Duration::from_millis(config.request_timeout_ms.get()),
    )
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
pub async fn health(State(state): State<Arc<ResolveState>>) -> Response {
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

/// Catch-all for unmatched paths, so `/favicon.ico` and friends get a body
/// rather than axum's default empty 404.
async fn not_found() -> AppError {
    AppError::NotFound
}

/// Build the standalone public router.
///
/// This is the whole self-hostable surface: one redirect route, one health
/// probe, rate limiting for callers with no API Gateway in front, and the same
/// middleware ordering `atpr-server` uses — trace outermost, then security
/// headers, then error pages, then the timeout, so the 504 the timeout
/// synthesises gets a page, headers on it, and a request log line.
pub fn router_with_state(state: Arc<ResolveState>) -> Router {
    Router::new()
        .route("/@{handle}/{code}", get(resolve))
        .route("/health", get(health))
        .fallback(not_found)
        .layer(GovernorLayer::new(client_ip_rate_limit(&state.config)))
        .layer(timeout_layer(&state.config))
        .layer(middleware::from_fn(error_page::html_errors))
        .layer(security_headers(hsts_header(&state.config)))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
