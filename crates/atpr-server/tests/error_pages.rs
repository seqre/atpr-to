//! Error responses through the real router: a page for browsers, JSON for the API.
//!
//! The unit tests in `src/api/error_page.rs` cover the negotiation decision on
//! its own. These drive the whole stack, because the parts most likely to break
//! are the ones the middleware does not own — whether it sits outside the
//! layers that synthesise their own responses, and whether the headers those
//! layers set survive the body swap.

use atpr_server::auth::FakeAuthenticator;
use atpr_server::router_with_state;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use wiremock::MockServer;

/// A router whose upstreams are unreachable — nothing here resolves a link.
async fn test_router() -> axum::Router {
    let mock = MockServer::start().await;
    let config = atpr_server::config::Config {
        slingshot_url: mock.uri(),
        ..atpr_server::config::Config::default()
    };
    let http = atpr_core::identity::http_client(&config);
    router_with_state(atpr_server::build_state_with(
        config,
        FakeAuthenticator::new("did:plc:testdid123"),
        http,
    ))
}

const BROWSER_ACCEPT: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";

async fn get(uri: &str, accept: Option<&str>) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut req = Request::builder().uri(uri);
    if let Some(a) = accept {
        req = req.header("accept", a);
    }
    let response = test_router()
        .await
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn test_browser_404_is_an_html_page() {
    let (status, headers, body) = get("/nope", Some(BROWSER_ACCEPT)).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        headers.get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    assert!(body.contains("<!DOCTYPE html>"), "body: {body}");
    // A cached 404 outlives the link that would fix it.
    assert_eq!(headers.get("cache-control").unwrap(), "no-store");
    // The body swap rebuilds the response; the security headers must survive it.
    assert!(headers
        .get("content-security-policy")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|c| c.contains("default-src 'self'")));
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
}

/// Rewriting the body without fixing the length forwards a truncated response
/// through API Gateway.
#[tokio::test]
async fn test_html_error_content_length_matches_the_body() {
    let (_, headers, body) = get("/nope", Some(BROWSER_ACCEPT)).await;

    let declared: usize = headers
        .get("content-length")
        .expect("content-length must be present")
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(declared, body.len());
}

#[tokio::test]
async fn test_api_404_stays_json_for_a_browser() {
    let (status, headers, body) = get("/api/nope", Some(BROWSER_ACCEPT)).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers.get("content-type").unwrap(), "application/json");
    let json: serde_json::Value = serde_json::from_str(&body).expect("body: {body}");
    assert_eq!(json["error"], "not found");
}

/// A client that asked for nothing in particular gets JSON. Two existing tests
/// depend on this, and so does every curl.
#[tokio::test]
async fn test_no_accept_header_stays_json() {
    let (status, headers, body) = get("/nope", None).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers.get("content-type").unwrap(), "application/json");
    assert!(!body.is_empty());
}

/// axum rejects this before any handler runs, so it never passes through
/// `AppError` — it gets a page because the middleware keys on status.
#[tokio::test]
async fn test_405_is_an_html_page_for_a_browser() {
    let response = test_router()
        .await
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("accept", BROWSER_ACCEPT)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
}

#[tokio::test]
async fn test_missing_static_asset_is_an_html_page() {
    let (status, headers, _) = get("/static/nope.css", Some(BROWSER_ACCEPT)).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        headers.get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
}

/// The guard is on status, not on `Accept` — a 200 must come through untouched.
#[tokio::test]
async fn test_successful_pages_are_not_rewritten() {
    let (status, headers, body) = get("/", Some(BROWSER_ACCEPT)).await;

    assert_eq!(status, StatusCode::OK);
    assert!(headers.get("cache-control").is_none_or(|v| v != "no-store"));
    assert!(body.contains("<form"), "the home page should still render");
}

/// Upstream failure text goes to the logs, never to a client — the JSON path
/// has always been opaque and the page must not become the leak.
#[tokio::test]
async fn test_upstream_detail_does_not_reach_the_page() {
    let mock = MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(
            wiremock::ResponseTemplate::new(500).set_body_string("PDS on fire at 10.0.0.7:2583"),
        )
        .mount(&mock)
        .await;

    let config = atpr_server::config::Config {
        slingshot_url: mock.uri(),
        ..atpr_server::config::Config::default()
    };
    let http = atpr_core::identity::http_client(&config);
    let response = router_with_state(atpr_server::build_state_with(
        config,
        FakeAuthenticator::new("did:plc:testdid123"),
        http,
    ))
    .oneshot(
        Request::builder()
            .uri("/@alice.test/abc123/info")
            .header("accept", BROWSER_ACCEPT)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();

    // The status is incidental to what this test guards. Under test the relay's
    // 500 sends resolution to the direct path, where the handle does not
    // resolve, so this is a 404 page rather than a 502 one — either way it is a
    // rendered error page, and either way the upstream's words must not be on it.
    assert!(
        status.is_client_error() || status.is_server_error(),
        "got {status}"
    );
    assert!(body.contains("<!DOCTYPE html>"), "expected a page: {body}");
    assert!(!body.contains("10.0.0.7"), "body: {body}");
    assert!(!body.contains("on fire"), "body: {body}");
    assert!(!body.contains("2583"), "body: {body}");
}
