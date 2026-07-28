//! End-to-end resolution tests: the router driven against a wiremock Slingshot.

use std::sync::Arc;

use atpr_to::auth::FakeAuthenticator;
use atpr_to::{router_with_state, AppState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Build an AppState pointing Slingshot at the given mock server URL.
async fn test_state(slingshot_url: String) -> Arc<AppState<FakeAuthenticator>> {
    state_with_client(slingshot_url, atpr_to::http_client()).await
}

/// As `test_state`, but with a caller-supplied HTTP client.
///
/// Used to force a transport timeout without making the test sleep for the
/// production 5s budget.
async fn state_with_client(
    slingshot_url: String,
    http: reqwest::Client,
) -> Arc<AppState<FakeAuthenticator>> {
    let config = atpr_to::config::Config {
        slingshot_url,
        ..atpr_to::config::Config::default()
    };
    atpr_to::build_state_with(config, FakeAuthenticator::new("did:plc:testdid123"), http)
}

#[tokio::test]
async fn test_resolve_via_slingshot_happy_path() {
    let mock = MockServer::start().await;

    // Mock resolveHandle: handle → DID
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.identity.resolveHandle"))
        .and(query_param("handle", "alice.test"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "did": "did:plc:testdid123" })),
        )
        .mount(&mock)
        .await;

    // Mock getRecord: DID + code → link record
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .and(query_param("repo", "did:plc:testdid123"))
        .and(query_param("collection", "to.atpr.link"))
        .and(query_param("rkey", "abc123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "uri": "at://did:plc:testdid123/to.atpr.link/abc123",
            "cid": "bafycid",
            "value": {
                "$type": "to.atpr.link",
                "url": "https://example.com/target",
                "updatedAt": "2024-01-01T00:00:00Z"
            }
        })))
        .mount(&mock)
        .await;

    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/@alice.test/abc123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    let location = response.headers().get("location").unwrap();
    assert_eq!(location, "https://example.com/target");
}

#[tokio::test]
async fn test_resolve_slingshot_down_falls_back() {
    let mock = MockServer::start().await;

    // Slingshot always returns 500
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;

    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/@alice.bsky.social/abc123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should not crash or return 405. Direct path will also fail
    // (no real DNS in test), so we expect a 4xx/5xx error page — not a panic.
    assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_ne!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    // Service degrades gracefully: 404 (not found) or 502 (bad gateway)
    assert!(
        response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::BAD_GATEWAY,
        "expected 404 or 502, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_resolve_record_not_found() {
    let mock = MockServer::start().await;

    // resolveHandle succeeds
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.identity.resolveHandle"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "did": "did:plc:testdid123" })),
        )
        .mount(&mock)
        .await;

    // getRecord returns 404
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock)
        .await;

    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/@alice.test/doesnotexist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // TODO(rebrand): body was asserted to be an HTML error page; error responses
    // are plain text until `templates/error.html` comes back (see src/error.rs).
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Regression test for the `contains("404")` misclassification.
///
/// The short code is `promo404`. `reqwest` appends ` for url ({url})` to the
/// Display of a send failure, and the getRecord URL carries `rkey=promo404` —
/// so a *timeout* formatted as
/// "error sending request for url (…rkey=promo404)" satisfied the old
/// `slingshot_err.to_string().contains("404")` check. The response became
/// 404 "Link not found", and the direct PDS fallback was skipped entirely
/// because the caller believed the answer was authoritative.
///
/// Verified against the pre-fix code: this returned 404. Correct behaviour is
/// to attempt the fallback and, when that also fails (no route to a real PDS
/// under test), return 502.
///
/// Note the failure must be at the transport layer. reqwest attaches the URL to
/// send errors but not to decode errors, and an HTTP status error is formatted
/// by us without the URL — neither of those reproduces the bug.
#[tokio::test]
async fn test_code_containing_404_is_not_treated_as_not_found() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.identity.resolveHandle"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "did": "did:plc:testdid123" })),
        )
        .mount(&mock)
        .await;

    // Hang past the client timeout: a transport failure, not a not-found.
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(30)))
        .mount(&mock)
        .await;

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(300))
        .build()
        .unwrap();
    let state = state_with_client(mock.uri(), http).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/@alice.test/promo404")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_GATEWAY,
        "a transport error on a code containing '404' must not be reported as not-found"
    );
}

/// A genuine 404 from the record hop still short-circuits, and is not confused
/// with a handle that failed to resolve.
#[tokio::test]
async fn test_handle_not_found_is_404() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.identity.resolveHandle"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock)
        .await;

    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/@alice.test/abc123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Mount a Slingshot that resolves a handle and serves a record with the given
/// destination URL, however dangerous.
async fn mock_serving_record_url(destination: &str) -> MockServer {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.identity.resolveHandle"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "did": "did:plc:testdid123" })),
        )
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "uri": "at://did:plc:testdid123/to.atpr.link/evil",
            "cid": "bafycid",
            "value": {
                "$type": "to.atpr.link",
                "url": destination,
                "updatedAt": "2024-01-01T00:00:00Z"
            }
        })))
        .mount(&mock)
        .await;

    mock
}

/// Regression test for bug #3, the redirect half.
///
/// A repo is user-writable: anyone can `putRecord` a `javascript:` URL to their
/// own PDS. The scheme was checked on the write path and not on the read path,
/// so such a record went straight into `Redirect::temporary`.
#[tokio::test]
async fn test_dangerous_scheme_is_not_redirected_to() {
    for destination in [
        "javascript:alert(document.cookie)",
        "data:text/html,<script>alert(1)</script>",
        "file:///etc/passwd",
    ] {
        let mock = mock_serving_record_url(destination).await;
        let state = test_state(mock.uri()).await;
        let app = router_with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/@alice.test/evil")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{destination} must not resolve"
        );
        assert!(
            response.headers().get("location").is_none(),
            "{destination} must not produce a Location header"
        );
    }
}

/// Regression test for bug #3, the info-page half.
///
/// `info.html` renders the destination into an `<a href>`. Askama escapes
/// characters, not schemes, so escaping was never a defence here.
#[tokio::test]
async fn test_dangerous_scheme_never_reaches_an_href() {
    let mock = mock_serving_record_url("javascript:alert(document.cookie)").await;
    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/@alice.test/evil/info")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(
        !body.contains("javascript:"),
        "destination scheme leaked into the rendered page: {body}"
    );
}

/// The read path must still accept ordinary destinations.
#[tokio::test]
async fn test_https_destination_still_resolves() {
    let mock = mock_serving_record_url("https://example.com/target").await;
    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/@alice.test/evil")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers().get("location").unwrap(),
        "https://example.com/target"
    );
}

/// Regression test for the missing `.fallback()` route: unmatched paths used to
/// return a bodyless 404.
#[tokio::test]
async fn test_unmatched_path_has_a_body() {
    let mock = MockServer::start().await;
    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/favicon.ico")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(!body.is_empty(), "fallback 404 should carry a body");
}

#[tokio::test]
async fn test_resolve_invalid_handle() {
    let mock = MockServer::start().await;
    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                // single-label handle is invalid per AT Protocol
                .uri("/@notahandle/abc123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_info_page_happy_path() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.identity.resolveHandle"))
        .and(query_param("handle", "alice.test"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "did": "did:plc:testdid123" })),
        )
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "uri": "at://did:plc:testdid123/to.atpr.link/abc123",
            "cid": "bafycid",
            "value": {
                "$type": "to.atpr.link",
                "url": "https://example.com/target",
                "updatedAt": "2024-01-15T10:00:00Z"
            }
        })))
        .mount(&mock)
        .await;

    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/@alice.test/abc123/info")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // TODO(rebrand): this asserted the rendered info page carried the destination
    // URL, the inline QR <svg>, and the updated_at date. Restore those checks
    // against the new `templates/info.html` once it exists.
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_info_page_not_found() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.identity.resolveHandle"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "did": "did:plc:testdid123" })),
        )
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.getRecord"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock)
        .await;

    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/@alice.test/nosuchcode/info")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_info_page_slingshot_error() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;

    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/@alice.test/abc123/info")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}
