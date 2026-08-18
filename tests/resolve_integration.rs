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
    state_with_timeout_ms(slingshot_url, None).await
}

/// As `test_state`, but with a shorter outbound timeout.
///
/// Used to force a transport timeout without making the test sleep for the
/// production 5s budget.
async fn state_with_timeout_ms(
    slingshot_url: String,
    timeout_ms: Option<u64>,
) -> Arc<AppState<FakeAuthenticator>> {
    let mut config = atpr_to::config::Config {
        slingshot_url,
        ..atpr_to::config::Config::default()
    };
    if let Some(ms) = timeout_ms {
        config.http_timeout_ms = std::num::NonZeroU64::new(ms).expect("timeout must be nonzero");
    }
    let http = atpr_to::http_client(&config);
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

    // The redirect is the product and costs two upstream hops. Caching it is
    // the single highest-leverage thing on the hot path, and there was none.
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "public, max-age=60"
    );

    // Set on every response, including this one.
    assert_eq!(
        response.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert!(response
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("default-src 'self'"));
    assert!(
        response.headers().contains_key("strict-transport-security"),
        "the default base_url is https, so HSTS must be sent"
    );
}

/// A cached 404 outlives the link that would fix it: everyone who probed a
/// code before it existed keeps getting the 404 for the whole redirect TTL.
#[tokio::test]
async fn test_errors_are_not_cached() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.identity.resolveHandle"))
        .respond_with(ResponseTemplate::new(400).set_body_json(
            serde_json::json!({ "error": "InvalidRequest", "message": "Unable to resolve handle" }),
        ))
        .mount(&mock)
        .await;

    let state = state_with_timeout_ms(mock.uri(), Some(200)).await;
    let response = router_with_state(state)
        .oneshot(
            Request::builder()
                .uri("/@nobody.test/abc123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(response.status().is_client_error() || response.status().is_server_error());
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
}

/// The request timeout sits below the Lambda function timeout on purpose: over
/// there, overrunning kills the execution environment and produces no response
/// and no log line. 504 rather than 408 because the wait is always on an
/// upstream, never on a slow client.
#[tokio::test]
async fn test_request_timeout_returns_504() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(30)))
        .mount(&mock)
        .await;

    let mut config = atpr_to::config::Config {
        slingshot_url: mock.uri(),
        ..atpr_to::config::Config::default()
    };
    // Well above the request budget, so the server-side layer is unambiguously
    // what fires — not the outbound client timeout.
    config.http_timeout_ms = std::num::NonZeroU64::new(20_000).unwrap();
    config.request_timeout_ms = std::num::NonZeroU64::new(200).unwrap();
    let http = atpr_to::http_client(&config);
    let state =
        atpr_to::build_state_with(config, FakeAuthenticator::new("did:plc:testdid123"), http);

    let started = std::time::Instant::now();
    let response = router_with_state(state)
        .oneshot(
            Request::builder()
                .uri("/@alice.test/abc123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the timeout layer did not bound the request"
    );

    // The timeout synthesises this response itself, so it only carries the
    // security headers because that layer sits outside the timeout rather than
    // inside it. A 504 is as likely to be seen by a stranger's browser as any
    // other status, and it used to go out bare.
    let headers = response.headers();
    assert!(
        headers
            .get("content-security-policy")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|csp| csp.contains("default-src 'self'")),
        "the timeout's 504 must carry the CSP"
    );
    assert_eq!(
        headers.get("x-content-type-options").unwrap(),
        "nosniff",
        "the timeout's 504 must carry nosniff"
    );
}

/// The timeout's 504 is synthesised by a layer, not by a handler, so it only
/// becomes a page because the negotiation middleware sits outside that layer.
#[tokio::test]
async fn test_request_timeout_is_an_html_page_for_a_browser() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(30)))
        .mount(&mock)
        .await;

    let mut config = atpr_to::config::Config {
        slingshot_url: mock.uri(),
        ..atpr_to::config::Config::default()
    };
    config.http_timeout_ms = std::num::NonZeroU64::new(20_000).unwrap();
    config.request_timeout_ms = std::num::NonZeroU64::new(200).unwrap();
    let http = atpr_to::http_client(&config);
    let state =
        atpr_to::build_state_with(config, FakeAuthenticator::new("did:plc:testdid123"), http);

    let response = router_with_state(state)
        .oneshot(
            Request::builder()
                .uri("/@alice.test/abc123")
                .header(
                    "accept",
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("<!DOCTYPE html>"), "body: {body}");
}

/// HSTS from a loopback dev server is worse than useless: a browser that
/// honours it makes every other local http service on the host unreachable.
#[tokio::test]
async fn test_loopback_does_not_send_hsts() {
    let mock = MockServer::start().await;
    let mut config = atpr_to::config::Config {
        slingshot_url: mock.uri(),
        ..atpr_to::config::Config::default()
    };
    config.base_url = atpr_to::config::BaseUrl::parse("http://localhost:9000").unwrap();
    let http = atpr_to::http_client(&config);
    let state =
        atpr_to::build_state_with(config, FakeAuthenticator::new("did:plc:testdid123"), http);

    let response = router_with_state(state)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        !response.headers().contains_key("strict-transport-security"),
        "HSTS must not be sent from a loopback origin"
    );
    // The rest of the policy still applies.
    assert_eq!(
        response.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// The shape a missing record actually arrives in.
///
/// XRPC does not use the status line for this: a real PDS and the relay both
/// answer `getRecord` for a missing rkey with **400** and
/// `{"error":"RecordNotFound"}` — measured against `pds.rip` and
/// `slingshot.microcosm.blue`, not assumed. The resolvers only checked for 404,
/// so every dead link was classified as an upstream fault and served as a 502
/// telling the visitor the network was not answering.
///
/// The mock returns the exact body both of them return.
#[tokio::test]
async fn test_xrpc_400_record_not_found_is_a_404_not_a_502() {
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
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "RecordNotFound",
            "message": "Could not locate record: at://did:plc:testdid123/to.atpr.link/doesnotexist"
        })))
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

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a named RecordNotFound is authoritative, whatever status carries it"
    );
}

/// A 400 that is *not* a missing record must not be read as one, or the fix
/// above would turn every relay malfunction into a confident "no such link".
///
/// Asserted on the body rather than the status for the same reason as
/// `test_code_containing_404_is_not_treated_as_not_found`: the relay's
/// malfunction sends us to the fallback, and under test the fallback cannot
/// resolve `alice.test`, so the honest end state is "no such account". What
/// must never appear is a bare `not found`, which would mean the relay's 400
/// had been taken for a missing record. The classification itself is asserted
/// directly in `resolver::tests`.
#[tokio::test]
async fn test_other_xrpc_400s_are_not_read_as_a_missing_record() {
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
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "InvalidRequest",
            "message": "something else entirely"
        })))
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

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&bytes);
    assert!(
        !body.contains(r#""error":"not found""#),
        "a relay 400 that names something other than RecordNotFound was taken \
         for a missing record: {body}"
    );
}

/// The handle half of not-found, stated on its own rather than left to be
/// inferred from the tests above.
///
/// jacquard reports `HandleResolutionExhausted` once DNS, the well-known
/// document and the PDS fallback have all been tried. That used to be an
/// upstream fault, so mistyping a handle produced a 502 telling the visitor the
/// network was down. It is a 404 now, and it says which of the two 404s it is.
#[tokio::test]
async fn test_an_unresolvable_handle_is_a_404_that_names_the_handle() {
    let mock = MockServer::start().await;

    // The relay cannot answer, so resolution falls through to the direct path,
    // where the handle itself is what fails.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;

    let state = test_state(mock.uri()).await;
    let app = router_with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                // `.test` like the rest of this file: syntactically a handle,
                // resolvable by nothing. A reserved TLD such as `.invalid` is
                // rejected by `Handle` parsing and never reaches resolution.
                .uri("/@nobody-has-this-handle.test/abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        json["error"], "handle not found",
        "a 404 for a handle must be distinguishable from a 404 for a link"
    );
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
/// Verified against the pre-fix code: this returned 404 with `not found`.
///
/// The assertion moved once an unresolvable handle became its own 404. Both
/// outcomes are now 404, so the status alone can no longer tell the two apart —
/// but the *body* can, and more precisely than the status ever did:
///
/// - `not found` means the slingshot error was string-matched into
///   `RecordNotFound` and the fallback was skipped. That is the bug.
/// - `handle not found` can only be produced by the direct resolver's own
///   handle lookup, so it is proof the fallback was actually attempted.
///
/// `alice.test` does not resolve under test, which is why the fallback's honest
/// verdict is "no such account".
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

    let state = state_with_timeout_ms(mock.uri(), Some(300)).await;
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

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&bytes);
    assert!(
        body.contains("handle not found"),
        "the fallback must be attempted; a body of `not found` would mean the \
         slingshot transport error was string-matched into a record-not-found. \
         got: {body}"
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
        "the last-modified date must appear"
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
