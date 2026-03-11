use std::sync::Arc;

use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;
use jacquard::api::com_atproto::repo::list_records::ListRecords;
use jacquard_common::types::collection::Collection;
use jacquard_common::types::did::Did;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::xrpc::XrpcClient;

use crate::auth::parse_session_cookie;
use crate::error;
use crate::generated::to_atpr::link::Link;
use crate::links::rkey_from_at_uri;
use crate::AppState;

/// A single link entry for display in the dashboard.
pub struct LinkEntry {
    /// The short code (record key).
    pub code: String,
    /// The destination URL.
    pub url: String,
    /// Last-modified timestamp (ISO 8601).
    pub updated_at: String,
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    handle: String,
    links: Vec<LinkEntry>,
}

/// Serve the home page with the login form.
///
/// If a valid session cookie is already present, redirects to `/dashboard`.
pub async fn home(jar: CookieJar) -> Response {
    if parse_session_cookie(&jar).is_some() {
        return Redirect::to("/dashboard").into_response();
    }
    let tmpl = HomeTemplate {};
    match tmpl.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => error::internal_error(&format!("Template error: {e}")),
    }
}

/// Serve the user dashboard (authenticated).
///
/// Redirects to `/` if the session cookie is missing or invalid.
#[tracing::instrument(skip_all)]
// coverage:excl-start
pub async fn dashboard(State(state): State<Arc<AppState>>, jar: CookieJar) -> Response {
    let (did_str, session_id) = match parse_session_cookie(&jar) {
        Some(p) => p,
        None => return Redirect::to("/").into_response(),
    };

    let did = match Did::new(&did_str) {
        Ok(d) => d,
        Err(_) => return Redirect::to("/").into_response(),
    };

    let session = match state.oauth.restore(&did, &session_id).await {
        Ok(s) => s,
        Err(_) => {
            // Clear the stale cookie to avoid an infinite redirect loop.
            let jar = jar.remove(Cookie::from("session"));
            return (jar, Redirect::to("/")).into_response();
        }
    };

    let owned_did = match Did::new(&did_str) {
        Ok(d) => d,
        Err(_) => return error::unauthorized("Invalid DID in session"),
    };

    let request = ListRecords::new()
        .repo(AtIdentifier::Did(owned_did))
        .collection(<Link as Collection>::NSID.to_string())
        .build();

    let links = match session.send(request).await {
        Ok(raw) => match raw.into_output() {
            Ok(output) => output
                .records
                .iter()
                .map(|record| {
                    let code = rkey_from_at_uri(record.uri.as_ref()).to_string();
                    let value =
                        serde_json::to_value(&record.value).unwrap_or(serde_json::Value::Null);
                    LinkEntry {
                        code,
                        url: value
                            .get("url")
                            .and_then(|u| u.as_str())
                            .unwrap_or("")
                            .to_string(),
                        updated_at: value
                            .get("updatedAt")
                            .and_then(|c| c.as_str())
                            .unwrap_or("")
                            .to_string(),
                    }
                })
                .collect(),
            Err(_) => vec![],
        },
        Err(_) => vec![],
    };

    let handle =
        crate::shorten::resolve_did_to_handle(&state.http, &state.config.slingshot_url, &did_str)
            .await
            .unwrap_or(did_str);

    let tmpl = DashboardTemplate { handle, links };
    match tmpl.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => error::internal_error(&format!("Template error: {e}")),
    }
}
// coverage:excl-stop

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::router;

    #[tokio::test]
    async fn test_home_returns_html_with_form() {
        let app = router();
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(html.contains("<form"), "expected <form in body");
        assert!(html.contains("<!DOCTYPE html>"), "expected HTML document");
    }

    #[tokio::test]
    async fn test_dashboard_redirects_without_auth() {
        let app = router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/dashboard")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response.headers().get("location").unwrap();
        assert_eq!(location, "/");
    }
}
