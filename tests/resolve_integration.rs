use std::sync::Arc;

use atpr_to::{router_with_state, AppState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Build an AppState pointing Slingshot at the given mock server URL.
async fn test_state(slingshot_url: String) -> Arc<AppState> {
    Arc::new(AppState {
        oauth: atpr_to::auth::build_oauth_client(),
        http: reqwest::Client::new(),
        slingshot_url,
    })
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
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "uri": "at://did:plc:testdid123/to.atpr.link/abc123",
                "cid": "bafycid",
                "value": {
                    "$type": "to.atpr.link",
                    "url": "https://example.com/target",
                    "createdAt": "2024-01-01T00:00:00Z"
                }
            })),
        )
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
        response.status() == StatusCode::NOT_FOUND
            || response.status() == StatusCode::BAD_GATEWAY,
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    // Should be an HTML error page
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("<!DOCTYPE html>"));
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("<!DOCTYPE html>"));
}
