//! The public pages only `atpr-server` mounts: the `/info` preview page.
//!
//! Resolution itself is covered in `atpr-core`'s integration suite; these
//! tests exist for what the preview page adds on top — rendering, and the
//! guarantee that a dangerous destination can never reach an `<a href>`.

use std::sync::Arc;

use atpr_server::auth::FakeAuthenticator;
use atpr_server::{router_with_state, AppState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Build an AppState pointing Slingshot at the given mock server URL.
async fn test_state(slingshot_url: String) -> Arc<AppState<FakeAuthenticator>> {
    let config = atpr_core::config::Config {
        slingshot_url,
        ..atpr_core::config::Config::default()
    };
    let http = atpr_core::identity::http_client(&config);
    atpr_server::build_state_with(config, FakeAuthenticator::new("did:plc:testdid123"), http)
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

/// The same guarantee on the other body a browser can get here.
///
/// The request above sends no `Accept`, so it is answered with JSON. A real
/// browser sends one and is answered with `templates/error.html` instead —
/// a second rendering path, reached by exactly the visitor this matters for.
#[tokio::test]
async fn test_dangerous_scheme_never_reaches_an_href_as_html() {
    let mock = mock_serving_record_url("javascript:alert(document.cookie)").await;
    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/@alice.test/evil/info")
                .header(
                    "accept",
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8",
        "a browser should be getting the page, or this test proves nothing"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(
        !body.contains("javascript:"),
        "destination scheme leaked into the error page: {body}"
    );
}

#[tokio::test]
async fn test_info_page_happy_path() {
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

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).expect("the page is UTF-8");

    // Restored: these assertions were deleted with the old frontend and left as
    // a TODO. They are about what the page *carries*, not how it looks, so they
    // do not pin the redesign to any particular markup.
    assert!(
        html.contains("https://example.com/target"),
        "the destination must appear on the page"
    );
    assert!(
        html.contains("<svg"),
        "the QR code must be rendered inline, not linked"
    );
    assert!(
        html.contains("2024-01-15T10:00:00Z"),
        "the machine-readable date must survive, in `datetime`"
    );
    assert!(
        html.contains("15 January 2024"),
        "and the visible date must be one a person would read: {html}"
    );
    assert!(
        html.contains("alice.test") && html.contains("abc123"),
        "the short link's own identity must appear"
    );
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

    // Not 200, and not a panic: the preview page has to fail as an error rather
    // than render half a link. Which error depends on how far resolution gets —
    // under test the relay's 500 sends it to the direct path, where
    // `alice.test` does not resolve, so it lands on the handle 404.
    assert!(
        response.status().is_client_error() || response.status().is_server_error(),
        "got {}",
        response.status()
    );
}

/// The health probe mounted under the JSON API prefix. `atpr-core` pins the
/// probe's status-code contract against a bare `ResolveState`; this drives it
/// through the server's `AppState` adapter, so the delegation cannot silently
/// break.
#[tokio::test]
async fn test_health_route_through_the_app_state_adapter() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "did": "did:plc:testdid123" })),
        )
        .mount(&mock)
        .await;

    let state = test_state(mock.uri()).await;
    let response = router_with_state(state)
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
}
