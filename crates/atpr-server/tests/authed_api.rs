//! The authenticated API, driven end to end against a fake authenticator.
//!
//! None of this was testable before the ports existed: handlers called
//! jacquard's `OAuthSession` inline, so `shorten`, `delete_link`, `list_links`
//! and the OAuth callback all needed a live PDS. All four were annotated
//! `coverage:excl` rather than covered.

use std::sync::Arc;

use atpr_server::auth::FakeAuthenticator;
use atpr_server::store::InMemoryLinkStore;
use atpr_server::{router_with_state, AppState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DID: &str = "did:plc:testdid123";

/// A Slingshot that resolves our DID back to a handle, which `shorten` needs to
/// build the short URL, doubling as the AppView the dashboard asks for an
/// avatar.
async fn mock_slingshot() -> MockServer {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/xrpc/com.atproto.repo.describeRepo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "handle": "alice.test", "did": DID })),
        )
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/xrpc/app.bsky.actor.getProfile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({ "did": DID, "avatar": "https://cdn.test/avatar.jpg" }),
        ))
        .mount(&mock)
        .await;
    mock
}

struct Harness {
    state: Arc<AppState<FakeAuthenticator>>,
    cookie: String,
    store: Arc<InMemoryLinkStore>,
    _mock: MockServer,
}

impl Harness {
    async fn new() -> Self {
        Self::with_auth(FakeAuthenticator::new(DID)).await
    }

    async fn with_auth(auth: FakeAuthenticator) -> Self {
        let mock = mock_slingshot().await;
        let config = atpr_server::config::Config {
            slingshot_url: mock.uri(),
            appview_url: mock.uri(),
            ..atpr_server::config::Config::default()
        };
        let cookie = auth.cookie_header();
        let store = Arc::clone(&auth.store);
        let http = atpr_core::identity::http_client(&config);
        let state = atpr_server::build_state_with(config, auth, http);
        Self {
            state,
            cookie,
            store,
            _mock: mock,
        }
    }

    fn router(&self) -> axum::Router {
        router_with_state(Arc::clone(&self.state))
    }

    async fn send(&self, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let response = self.router().oneshot(req).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    fn authed(&self, method: &str, uri: &str, body: Option<serde_json::Value>) -> Request<Body> {
        let builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("cookie", &self.cookie);

        match body {
            Some(b) => builder
                .header("content-type", "application/json")
                .body(Body::from(b.to_string()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        }
    }
}

#[tokio::test]
async fn test_shorten_happy_path() {
    let h = Harness::new().await;
    let (status, json) = h
        .send(h.authed(
            "POST",
            "/api/shorten",
            Some(serde_json::json!({ "url": "https://example.com/target", "code": "mycode" })),
        ))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["short_url"], "https://atpr.to/@alice.test/mycode");
    assert_eq!(
        h.store.get("mycode").as_deref(),
        Some("https://example.com/target")
    );
}

#[tokio::test]
async fn test_shorten_generates_a_code_when_absent() {
    let h = Harness::new().await;
    let (status, json) = h
        .send(h.authed(
            "POST",
            "/api/shorten",
            Some(serde_json::json!({ "url": "https://example.com/target" })),
        ))
        .await;

    assert_eq!(status, StatusCode::OK);
    let short_url = json["short_url"].as_str().unwrap();
    assert!(short_url.starts_with("https://atpr.to/@alice.test/"));
    assert_eq!(h.store.len(), 1);
}

#[tokio::test]
async fn test_shorten_rejects_invalid_code() {
    let h = Harness::new().await;
    let (status, json) = h
        .send(h.authed(
            "POST",
            "/api/shorten",
            Some(serde_json::json!({ "url": "https://example.com", "code": "has spaces" })),
        ))
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"].as_str().unwrap().contains("short code"));
    assert!(h.store.is_empty(), "nothing should be written");
}

#[tokio::test]
async fn test_shorten_rejects_dangerous_scheme() {
    let h = Harness::new().await;
    for url in [
        "javascript:alert(1)",
        "data:text/html,x",
        "file:///etc/passwd",
    ] {
        let (status, _) = h
            .send(h.authed(
                "POST",
                "/api/shorten",
                Some(serde_json::json!({ "url": url })),
            ))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url} must be rejected");
    }
    assert!(h.store.is_empty());
}

#[tokio::test]
async fn test_shorten_rejects_overlong_url() {
    let h = Harness::new().await;
    let long = format!("https://example.com/{}", "a".repeat(2100));
    let (status, _) = h
        .send(h.authed(
            "POST",
            "/api/shorten",
            Some(serde_json::json!({ "url": long })),
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// Bug #4: re-using a code used to silently overwrite the previous link,
/// because `putRecord` is an upsert and nothing checked first.
#[tokio::test]
async fn test_shorten_collision_is_409_and_preserves_the_original() {
    let h = Harness::new().await;

    let (first, _) = h
        .send(h.authed(
            "POST",
            "/api/shorten",
            Some(serde_json::json!({ "url": "https://first.example", "code": "dup" })),
        ))
        .await;
    assert_eq!(first, StatusCode::OK);

    let (second, json) = h
        .send(h.authed(
            "POST",
            "/api/shorten",
            Some(serde_json::json!({ "url": "https://second.example", "code": "dup" })),
        ))
        .await;

    assert_eq!(second, StatusCode::CONFLICT);
    assert_eq!(json["error"], "short code already in use");
    assert_eq!(
        h.store.get("dup").as_deref(),
        Some("https://first.example/"),
        "the original link must survive the collision"
    );
}

#[tokio::test]
async fn test_shorten_upstream_failure_is_502() {
    let h = Harness::with_auth(
        FakeAuthenticator::new(DID).with_store(InMemoryLinkStore::failing("PDS unreachable")),
    )
    .await;

    let (status, json) = h
        .send(h.authed(
            "POST",
            "/api/shorten",
            Some(serde_json::json!({ "url": "https://example.com" })),
        ))
        .await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(json["error"], "upstream unavailable");
    assert!(
        !json.to_string().contains("PDS unreachable"),
        "upstream detail must not leak"
    );
}

/// Bug #2: when handle resolution failed after the record was written,
/// `shorten` fell back to the DID and returned `.../@did:plc:xyz/code`.
/// `resolve` parses that segment with `Handle::new_owned` and a DID is not a
/// valid handle, so the user was handed a permanently dead link and told it
/// succeeded.
#[tokio::test]
async fn test_shorten_errors_rather_than_minting_a_did_url() {
    // A Slingshot that cannot answer describeRepo, so no handle is available.
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;

    let config = atpr_server::config::Config {
        slingshot_url: mock.uri(),
        // Keep the DID-document fallback from doing real DNS.
        http_timeout_ms: std::num::NonZeroU64::new(200).unwrap(),
        ..atpr_server::config::Config::default()
    };
    let auth = FakeAuthenticator::new(DID);
    let cookie = auth.cookie_header();
    let http = atpr_core::identity::http_client(&config);
    let state = atpr_server::build_state_with(config, auth, http);

    let response = router_with_state(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/shorten")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"url":"https://example.com","code":"dead"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_GATEWAY,
        "an unresolvable handle must be an error, not a dead link"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(
        !body.contains("did:plc:"),
        "the response must not contain a DID-based short URL: {body}"
    );
    assert!(!body.contains("short_url"), "no short URL may be returned");
}

/// Bug #10: there were no HTTP timeouts anywhere, so the only bound on a hung
/// upstream was Lambda's 30s function timeout.
#[tokio::test]
async fn test_hung_upstream_is_bounded_by_the_client_timeout() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(30)))
        .mount(&mock)
        .await;

    let config = atpr_server::config::Config {
        slingshot_url: mock.uri(),
        http_timeout_ms: std::num::NonZeroU64::new(200).unwrap(),
        ..atpr_server::config::Config::default()
    };
    let http = atpr_core::identity::http_client(&config);
    let state = atpr_server::build_state_with(config, FakeAuthenticator::new(DID), http);

    let started = std::time::Instant::now();
    let response = router_with_state(state)
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the request must be bounded by the client timeout, took {:?}",
        started.elapsed()
    );
}

/// Bug #7: the limiter used one global bucket, so a single noisy client could
/// deny login to everyone on the instance. It is now keyed per client IP.
#[tokio::test]
async fn test_rate_limit_is_per_client_ip() {
    let h = Harness::new().await;
    // One router, so one limiter: `Harness::router` builds a fresh
    // `GovernorLayer` each call, which would hand every request its own
    // untouched bucket.
    let app = h.router();

    // Burst is 10. One client exhausts its own bucket...
    let mut saw_429 = false;
    for _ in 0..25 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/links")
                    .header("x-forwarded-for", "203.0.113.1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            saw_429 = true;
            break;
        }
    }
    assert!(saw_429, "a client exceeding the burst must be limited");

    // ...without affecting anyone else. Under the old global key this would
    // have been 429 too.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/links")
                .header("x-forwarded-for", "198.51.100.7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "a different client must still be served (401 for want of a cookie, not 429)"
    );
}

#[tokio::test]
async fn test_shorten_requires_auth() {
    let h = Harness::new().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/shorten")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"url":"https://example.com"}"#))
        .unwrap();
    let (status, _) = h.send(req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_delete_removes_the_link() {
    let h = Harness::new().await;
    h.store.insert("gone", "https://example.com");

    let (status, _) = h.send(h.authed("DELETE", "/api/shorten/gone", None)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(h.store.is_empty());
}

#[tokio::test]
async fn test_delete_missing_is_404() {
    let h = Harness::new().await;
    let (status, json) = h
        .send(h.authed("DELETE", "/api/shorten/nosuchcode", None))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

/// The point of the route: a link can be repointed without losing its code.
#[tokio::test]
async fn test_update_repoints_an_existing_link() {
    let h = Harness::new().await;
    h.store.insert("docs", "https://old.example/page");

    let (status, _) = h
        .send(h.authed(
            "PUT",
            "/api/shorten/docs",
            Some(serde_json::json!({ "url": "https://new.example/page" })),
        ))
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        h.store.get("docs").as_deref(),
        Some("https://new.example/page")
    );
}

/// `POST` refuses to overwrite and `PUT` agrees to — that difference is the
/// whole reason this route exists, so it is worth pinning rather than leaving
/// as an implication of two separate tests.
#[tokio::test]
async fn test_post_still_refuses_to_overwrite_what_put_replaces() {
    let h = Harness::new().await;
    h.store.insert("docs", "https://original.example/");

    let (post_status, _) = h
        .send(h.authed(
            "POST",
            "/api/shorten",
            Some(serde_json::json!({ "url": "https://sneaky.example/", "code": "docs" })),
        ))
        .await;
    assert_eq!(post_status, StatusCode::CONFLICT);
    assert_eq!(
        h.store.get("docs").as_deref(),
        Some("https://original.example/"),
        "a refused POST must leave the original alone"
    );

    let (put_status, _) = h
        .send(h.authed(
            "PUT",
            "/api/shorten/docs",
            Some(serde_json::json!({ "url": "https://deliberate.example/" })),
        ))
        .await;
    assert_eq!(put_status, StatusCode::NO_CONTENT);
    assert_eq!(
        h.store.get("docs").as_deref(),
        Some("https://deliberate.example/")
    );
}

/// Read-before-write would cost a round trip and still race, so a code with no
/// record is created rather than rejected.
#[tokio::test]
async fn test_update_creates_a_missing_code() {
    let h = Harness::new().await;

    let (status, _) = h
        .send(h.authed(
            "PUT",
            "/api/shorten/fresh",
            Some(serde_json::json!({ "url": "https://example.com/" })),
        ))
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        h.store.get("fresh").as_deref(),
        Some("https://example.com/")
    );
}

/// The read path validates schemes too, but a dangerous destination should
/// never reach the record in the first place.
#[tokio::test]
async fn test_update_rejects_a_dangerous_scheme() {
    let h = Harness::new().await;
    h.store.insert("docs", "https://safe.example/");

    let (status, json) = h
        .send(h.authed(
            "PUT",
            "/api/shorten/docs",
            Some(serde_json::json!({ "url": "javascript:alert(1)" })),
        ))
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"].as_str().unwrap().contains("http"));
    assert_eq!(
        h.store.get("docs").as_deref(),
        Some("https://safe.example/"),
        "a rejected update must not disturb the record"
    );
}

#[tokio::test]
async fn test_update_requires_auth() {
    let h = Harness::new().await;
    h.store.insert("docs", "https://safe.example/");

    let response = h
        .router()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/shorten/docs")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "url": "https://evil.example/" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        h.store.get("docs").as_deref(),
        Some("https://safe.example/")
    );
}

#[tokio::test]
async fn test_delete_rejects_invalid_code() {
    let h = Harness::new().await;
    let (status, _) = h
        .send(h.authed("DELETE", "/api/shorten/has.dot", None))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_returns_stored_links() {
    let h = Harness::new().await;
    h.store.insert("one", "https://one.example");
    h.store.insert("two", "https://two.example");

    let (status, json) = h.send(h.authed("GET", "/api/links", None)).await;
    assert_eq!(status, StatusCode::OK);

    let links = json["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!(links[0]["code"], "one");
    assert_eq!(links[0]["url"], "https://one.example/");
}

/// Bug #5: `listRecords` sent no limit and discarded the returned cursor, so a
/// user with more links than one page silently lost the rest.
#[tokio::test]
async fn test_list_paginates_past_the_default_page_size() {
    let h = Harness::new().await;
    for i in 0..120 {
        h.store
            .insert(&format!("code{i:03}"), "https://example.com");
    }

    let (status, first) = h.send(h.authed("GET", "/api/links?limit=50", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["links"].as_array().unwrap().len(), 50);

    let cursor = first["cursor"]
        .as_str()
        .expect("a cursor must be returned when more links remain");

    let (_, second) = h
        .send(h.authed("GET", &format!("/api/links?limit=50&cursor={cursor}"), None))
        .await;
    assert_eq!(second["links"].as_array().unwrap().len(), 50);

    let cursor = second["cursor"].as_str().unwrap();
    let (_, third) = h
        .send(h.authed("GET", &format!("/api/links?limit=50&cursor={cursor}"), None))
        .await;
    assert_eq!(third["links"].as_array().unwrap().len(), 20);
    assert!(third["cursor"].is_null(), "last page has no cursor");
}

#[tokio::test]
async fn test_list_requires_auth() {
    let h = Harness::new().await;
    let req = Request::builder()
        .uri("/api/links")
        .body(Body::empty())
        .unwrap();
    let (status, _) = h.send(req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_login_redirects_to_the_authorization_server() {
    let h = Harness::new().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/login")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from("handle=alice.test"))
        .unwrap();

    let response = h.router().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("https://pds.test/oauth/authorize"));
}

#[tokio::test]
async fn test_login_upstream_failure_is_502() {
    let h = Harness::with_auth(FakeAuthenticator::new(DID).with_failing_login("PDS down")).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/login")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from("handle=alice.test"))
        .unwrap();
    let (status, json) = h.send(req).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(!json.to_string().contains("PDS down"));
}

/// The OAuth callback was `coverage:excl` because it could not be exercised.
/// Putting login into the `Authenticator` port makes it reachable.
#[tokio::test]
async fn test_oauth_callback_sets_a_session_cookie() {
    let h = Harness::new().await;
    let req = Request::builder()
        .uri("/oauth/callback?code=abc&state=xyz")
        .body(Body::empty())
        .unwrap();

    let response = h.router().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(response.headers().get("location").unwrap(), "/");

    let set_cookie = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(set_cookie.contains("session="), "{set_cookie}");
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(set_cookie.contains("Secure"), "{set_cookie}");
    assert!(set_cookie.contains("SameSite=Lax"), "{set_cookie}");
}

#[tokio::test]
async fn test_oauth_callback_failure_is_502() {
    let h = Harness::with_auth(FakeAuthenticator::new(DID).with_failing_login("bad code")).await;
    let req = Request::builder()
        .uri("/oauth/callback?code=abc")
        .body(Body::empty())
        .unwrap();
    let (status, json) = h.send(req).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(!json.to_string().contains("bad code"));
}

/// The cookie a callback issues must be one the extractor then accepts.
#[tokio::test]
async fn test_callback_cookie_authenticates_subsequent_requests() {
    let h = Harness::new().await;

    let response = h
        .router()
        .oneshot(
            Request::builder()
                .uri("/oauth/callback?code=abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let cookie = set_cookie.split(';').next().unwrap().to_string();

    let (status, _) = h
        .send(
            Request::builder()
                .uri("/api/links")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

/// Bug #6: logging out must revoke the session server-side, not merely drop the
/// cookie. The access token, refresh token and DPoP key used to stay live in the
/// store forever, so a cookie captured before "sign out" kept working after it.
#[tokio::test]
async fn test_logout_revokes_the_session_server_side() {
    let auth = FakeAuthenticator::new(DID);
    let revoked = Arc::clone(&auth.revoked);
    let h = Harness::with_auth(auth).await;

    assert!(
        !revoked.load(std::sync::atomic::Ordering::SeqCst),
        "precondition: not yet revoked"
    );

    let response = h
        .router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/logout")
                .header("cookie", &h.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        revoked.load(std::sync::atomic::Ordering::SeqCst),
        "logout must revoke the session, not just clear the cookie"
    );
}

/// The clearing cookie must match the attributes of the cookie it clears, or the
/// browser matches nothing and the session cookie survives "sign out".
#[tokio::test]
async fn test_logout_clearing_cookie_mirrors_attributes() {
    let h = Harness::new().await;
    let response = h
        .router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/logout")
                .header("cookie", &h.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let set_cookie = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();

    assert!(set_cookie.contains("Path=/"), "{set_cookie}");
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(set_cookie.contains("SameSite=Lax"), "{set_cookie}");
    assert!(
        set_cookie.contains("Max-Age=0") || set_cookie.contains("session=;"),
        "{set_cookie}"
    );
}

/// `__Host-` is free hardening: a browser refuses such a cookie unless it is
/// Secure, Path=/ and Domain-less, so a subdomain cannot overwrite it.
#[tokio::test]
async fn test_production_session_cookie_uses_the_host_prefix() {
    let mock = mock_slingshot().await;
    let config = atpr_server::config::Config {
        slingshot_url: mock.uri(),
        // The default base_url is https://atpr.to, i.e. not loopback.
        ..atpr_server::config::Config::default()
    };
    let http = atpr_core::identity::http_client(&config);
    let state = atpr_server::build_state_with(config, FakeAuthenticator::new(DID), http);

    let response = router_with_state(state)
        .oneshot(
            Request::builder()
                .uri("/oauth/callback?code=abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let set_cookie = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(set_cookie.starts_with("__Host-session="), "{set_cookie}");
    assert!(set_cookie.contains("Secure"), "{set_cookie}");
    assert!(!set_cookie.contains("Domain="), "{set_cookie}");
}

/// On loopback the plain name is used, because `__Host-` requires Secure and
/// local development is over http.
#[tokio::test]
async fn test_loopback_session_cookie_uses_the_plain_name() {
    let mock = mock_slingshot().await;
    let config = atpr_server::config::Config {
        slingshot_url: mock.uri(),
        base_url: atpr_server::config::BaseUrl::parse("http://127.0.0.1:9000").unwrap(),
        ..atpr_server::config::Config::default()
    };
    let http = atpr_core::identity::http_client(&config);
    let state = atpr_server::build_state_with(config, FakeAuthenticator::new(DID), http);

    let response = router_with_state(state)
        .oneshot(
            Request::builder()
                .uri("/oauth/callback?code=abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let set_cookie = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(set_cookie.starts_with("session="), "{set_cookie}");
    assert!(!set_cookie.contains("Secure"), "{set_cookie}");
}

/// `ui::home` used to treat cookie *presence* as authentication, so a client
/// holding any junk cookie was bounced to /dashboard and straight back.
#[tokio::test]
async fn test_home_with_junk_cookie_does_not_redirect() {
    let h = Harness::new().await;
    let response = h
        .router()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", "session=not-a-real|session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "an invalid session must render the login page, not redirect"
    );
}

#[tokio::test]
async fn test_home_with_valid_session_redirects_to_dashboard() {
    let h = Harness::new().await;
    let response = h
        .router()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", &h.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/dashboard");
}

/// The dashboard's authenticated path: resolve the handle, fetch the avatar,
/// render. Untestable while the AppView URL was hardcoded into the handler.
#[tokio::test]
async fn test_dashboard_renders_for_an_authenticated_user() {
    let h = Harness::new().await;
    let response = h
        .router()
        .oneshot(h.authed("GET", "/dashboard", None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    // The DID was resolved to a handle rather than rendered raw, and the
    // avatar came back from the AppView.
    assert!(body.contains("alice.test"), "body: {body}");
    assert!(body.contains("https://cdn.test/avatar.jpg"), "body: {body}");

    // The DID is now in the markup, in `data-did`, so the page can narrow its
    // Jetstream subscription to this repo. This assertion used to be "the DID
    // appears nowhere", which was a proxy for what it actually protects: that
    // the *visible* identity is the handle and not a raw identifier. That is
    // asserted directly now, on the element which carries the name.
    //
    // The DID itself is not a secret — it is the subject of every short link
    // this account has published, and one request from the handle — and this
    // page is served only to the account it belongs to.
    let whoami_start = body.find("whoami").expect("the identity block renders");
    let whoami = &body[whoami_start..whoami_start + 400.min(body.len() - whoami_start)];
    assert!(
        !whoami.contains(DID),
        "the visible identity must be the handle, not the DID: {whoami}"
    );
    assert!(
        body.contains(&format!(r#"data-did="{DID}""#)),
        "the DID must reach the script that subscribes for this repo: {body}"
    );
}

/// `ui.rs` names this as the top regression risk left by the frontend removal.
#[tokio::test]
async fn test_dashboard_redirects_without_auth() {
    let h = Harness::new().await;
    let response = h
        .router()
        .oneshot(
            Request::builder()
                .uri("/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/");
}

#[tokio::test]
async fn test_dashboard_clears_a_stale_cookie() {
    let h = Harness::new().await;
    let response = h
        .router()
        .oneshot(
            Request::builder()
                .uri("/dashboard")
                .header("cookie", "session=did:plc:someoneelse|stale")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .expect("the stale cookie must be cleared, or the redirect loops")
        .to_str()
        .unwrap();
    assert!(set_cookie.contains("session="), "{set_cookie}");
}
