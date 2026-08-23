//! Tests specific to the standalone redirect router: the routes and behaviours
//! only `atpr-redirect` (and `atpr-server`'s nested state) expose.

use std::sync::Arc;

use atpr_core::config::Config;
use atpr_core::identity::http_client;
use atpr_core::redirect::{router_with_state, ResolveState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Build a `ResolveState` from a config override.
fn state_with(config: Config) -> Arc<ResolveState> {
    let http = http_client(&config);
    Arc::new(ResolveState::new(config, http))
}

/// The health probe must key its status code on Slingshot's actual health:
/// uptime checks read the code, not the body — it once answered 200 while
/// reporting `"status":"degraded"` and nothing ever fired.
#[tokio::test]
async fn test_health_ok() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "did": "did:plc:testdid123" })),
        )
        .mount(&mock)
        .await;

    let app = router_with_state(state_with(Config {
        slingshot_url: mock.uri(),
        ..Config::default()
    }));
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

    let app = router_with_state(state_with(Config {
        slingshot_url: mock.uri(),
        ..Config::default()
    }));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // The status code must carry the failure: uptime checks key on it.
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "degraded");
    assert_eq!(json["slingshot"], "unreachable");
}

/// A self-hosted instance has no API Gateway in front of it, so the in-process
/// limiter is the only thing standing between one noisy client and a relay
/// that is rate-limiting *us*. It has to actually fire on the public route.
#[tokio::test]
async fn test_public_route_is_rate_limited() {
    let mock = MockServer::start().await;
    // An authoritative not-found: each request is cheap and never falls back.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock)
        .await;

    let config = Config {
        slingshot_url: mock.uri(),
        rate_limit: atpr_core::config::RateLimitConfig {
            per_second: std::num::NonZeroU64::new(1).expect("1 is nonzero"),
            burst_size: std::num::NonZeroU32::new(2).expect("2 is nonzero"),
        },
        ..Config::default()
    };

    let app = router_with_state(state_with(config));
    let mut saw_429 = false;
    for _ in 0..6 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/@alice.test/abc123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            saw_429 = true;
        }
    }
    assert!(
        saw_429,
        "six immediate requests against a 1/s burst-2 limiter must hit 429"
    );
}
