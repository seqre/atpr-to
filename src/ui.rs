// TODO(rebrand): this module's test suite was deleted with the old frontend.
// All 12 tests asserted on rendered markup (Pico/Alpine CDN strings, Alpine
// directives, `x-ref="qrDialog"`, placeholder copy, `width:100%`), so they
// would have pinned the redesign to the design being replaced.
//
// Coverage to write back, roughly in priority order:
//  1. `test_dashboard_redirects_without_auth` — GET /dashboard with no session
//     cookie => 303 with Location: /. This one was pure behaviour, not markup,
//     and is the only real regression risk right now. Restore it first.
//  2. GET / with a session cookie => 303 to /dashboard (was never covered).
//  3. GET /dashboard with a stale/unrestorable session => 303 to / *and* the
//     `session` cookie cleared, guarding the redirect loop that `ui::dashboard`
//     works around.
//  4. Once the new UI exists: that the login form still posts `handle` to
//     /api/login, asserted structurally rather than by substring match.
//
// `dashboard` is `coverage:excl` and needs live OAuth state, so a test-only
// `render_dashboard_template()` helper existed to render the template directly.
// It was removed with the tests; reintroduce it if the new suite needs it.

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

    let did: Did = match Did::new_owned(&did_str) {
        Ok(d) => d,
        Err(_) => return Redirect::to("/").into_response(),
    };

    if state.oauth.restore(&did, &session_id).await.is_err() {
        // Clear the stale cookie to avoid an infinite redirect loop.
        let jar = jar.remove(Cookie::from("session"));
        return (jar, Redirect::to("/")).into_response();
    }

    let (handle, avatar) = tokio::join!(
        crate::shorten::resolve_did_to_handle(
            &state.http,
            &state.resolver,
            &state.config.slingshot_url,
            &did_str
        ),
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
