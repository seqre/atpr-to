use std::sync::Arc;

use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;
use jacquard_common::types::did::Did;

use crate::auth::parse_session_cookie;
use crate::error;
use crate::AppState;

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    handle: String,
    /// Avatar URL, empty string if not available.
    avatar: String,
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

// coverage:excl-start
async fn fetch_bsky_avatar(client: &reqwest::Client, did: &str) -> Option<String> {
    let url = format!(
        "https://public.api.bsky.app/xrpc/app.bsky.actor.getProfile?actor={}",
        urlencoding::encode(did),
    );
    let body: serde_json::Value = client.get(&url).send().await.ok()?.json().await.ok()?;
    body.get("avatar")
        .and_then(|a| a.as_str())
        .map(str::to_owned)
}
// coverage:excl-stop

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

    if state.oauth.restore(&did, &session_id).await.is_err() {
        // Clear the stale cookie to avoid an infinite redirect loop.
        let jar = jar.remove(Cookie::from("session"));
        return (jar, Redirect::to("/")).into_response();
    }

    let (handle, avatar) = tokio::join!(
        crate::shorten::resolve_did_to_handle(&state.http, &state.config.slingshot_url, &did_str),
        fetch_bsky_avatar(&state.http, &did_str),
    );
    let handle = handle.unwrap_or(did_str);

    let tmpl = DashboardTemplate {
        handle,
        avatar: avatar.unwrap_or_default(),
    };
    match tmpl.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => error::internal_error(&format!("Template error: {e}")),
    }
}
// coverage:excl-stop

#[cfg(test)]
fn render_dashboard_template() -> String {
    use askama::Template;
    DashboardTemplate {
        handle: "test.bsky.social".to_string(),
        avatar: String::new(),
    }
    .render()
    .unwrap()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::router;

    async fn home_html() -> String {
        let app = router();
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8_lossy(&body).into_owned()
    }

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
    async fn test_home_alpine_identity_search() {
        let html = home_html().await;
        assert!(
            html.contains(r#"action="/api/login""#),
            "expected login form action"
        );
        assert!(html.contains("x-data"), "expected Alpine x-data");
        assert!(html.contains("x-show"), "expected Alpine x-show");
        assert!(html.contains("limit=3"), "expected limit=3 in fetch URL");
    }

    #[tokio::test]
    async fn test_home_has_pico_and_alpine() {
        let html = home_html().await;
        assert!(html.contains("pico.min.css"), "expected Pico CSS CDN link");
        assert!(html.contains("alpinejs"), "expected Alpine.js CDN link");
    }

    #[tokio::test]
    async fn test_home_button_text() {
        let html = home_html().await;
        assert!(
            html.contains("AT Protocol identity"),
            "expected AT Protocol identity button text"
        );
    }

    #[tokio::test]
    async fn test_home_debounce_150ms() {
        let html = home_html().await;
        assert!(
            html.contains("debounce.150ms"),
            "expected 150ms debounce"
        );
    }

    #[tokio::test]
    async fn test_home_rotating_placeholder() {
        let html = home_html().await;
        assert!(html.contains("placeholders:"), "expected placeholders array");
        assert!(html.contains(":placeholder=\"placeholder\""), "expected :placeholder binding");
        assert!(html.contains("eurosky.social"), "expected placeholder entries");
    }

    #[tokio::test]
    async fn test_home_has_main_container() {
        let html = home_html().await;
        assert!(html.contains("<main"), "expected <main element");
    }

    #[tokio::test]
    async fn test_dashboard_has_alpine_init() {
        let html = super::render_dashboard_template();
        assert!(html.contains("x-data"), "expected x-data");
        assert!(html.contains("x-init"), "expected x-init");
        assert!(html.contains("/api/links"), "expected /api/links fetch");
    }

    #[tokio::test]
    async fn test_dashboard_qr_dialog() {
        let html = super::render_dashboard_template();
        assert!(html.contains("<dialog"), "expected <dialog element");
        assert!(html.contains("showModal"), "expected showModal call");
        assert!(
            html.contains(r#"x-ref="qrDialog""#),
            "expected qrDialog ref"
        );
        assert!(html.contains("download"), "expected download button");
    }

    #[tokio::test]
    async fn test_dashboard_inline_edit() {
        let html = super::render_dashboard_template();
        assert!(html.contains("link.editing"), "expected edit mode x-show");
        assert!(
            html.contains("confirmEdit"),
            "expected confirm edit handler"
        );
        assert!(html.contains("Cancel"), "expected Cancel button");
    }

    #[tokio::test]
    async fn test_dashboard_action_buttons_grid() {
        let html = super::render_dashboard_template();
        assert!(html.contains("aria-busy"), "expected aria-busy on action buttons");
        assert!(html.contains("link.deleting"), "expected link.deleting binding");
        assert!(html.contains("link.saving"), "expected link.saving binding");
        assert!(html.contains("width:100%"), "expected full-width delete button");
        assert!(html.contains("width:20rem"), "expected wider search bar");
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
