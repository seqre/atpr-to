//! The HTML UI: home page and dashboard.
//!
//! This module's original test suite was deleted with the old frontend: all 12
//! tests asserted on rendered markup (Pico/Alpine CDN strings, Alpine
//! directives, placeholder copy, `width:100%`), so they would have pinned the
//! redesign to the design being replaced.
//!
//! The behavioural coverage that mattered is back, in `tests/authed_api.rs`,
//! written against behaviour rather than markup:
//! `test_dashboard_redirects_without_auth`,
//! `test_dashboard_clears_a_stale_cookie`,
//! `test_home_with_valid_session_redirects_to_dashboard` and
//! `test_home_with_junk_cookie_does_not_redirect`.
//!
//! Still to write once the new UI exists: that the login form posts `handle` to
//! `/api/login`, asserted structurally rather than by substring match.

use std::sync::Arc;

use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;

use crate::auth::{clear_session, Authenticator, MaybeAuth};
use crate::error::AppError;
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
/// Redirects to `/dashboard` when the caller has a session that actually
/// restores. Checking cookie *presence* — which is what this did — meant any
/// client holding a junk `session` cookie was bounced to `/dashboard`, which
/// bounced it straight back.
pub async fn home<A: Authenticator>(auth: MaybeAuth<A>) -> Result<Response, AppError> {
    if auth.is_authenticated() {
        return Ok(Redirect::to("/dashboard").into_response());
    }
    let html = HomeTemplate {}.render().map_err(AppError::internal)?;
    Ok(Html(html).into_response())
}

async fn fetch_bsky_avatar(
    client: &reqwest::Client,
    appview_url: &str,
    did: &str,
) -> Option<String> {
    let url = format!(
        "{}/xrpc/app.bsky.actor.getProfile?actor={}",
        appview_url.trim_end_matches('/'),
        urlencoding::encode(did),
    );
    let body: serde_json::Value = client.get(&url).send().await.ok()?.json().await.ok()?;
    body.get("avatar")
        .and_then(|a| a.as_str())
        .map(str::to_owned)
}

/// Serve the user dashboard (authenticated).
///
/// Redirects to `/` if the session cookie is missing or invalid.
#[tracing::instrument(skip_all)]
pub async fn dashboard<A: Authenticator>(
    State(state): State<Arc<AppState<A>>>,
    jar: CookieJar,
) -> Result<Response, AppError> {
    // This used to reimplement the auth extractor by hand — parse the cookie,
    // re-parse the DID, call `oauth.restore` — a third copy of logic that now
    // has one home.
    let Ok(user) = state.auth.authenticate(&jar).await else {
        // Clear the stale cookie to avoid an infinite redirect loop.
        let jar = clear_session(jar, &state.config.base_url);
        return Ok((jar, Redirect::to("/")).into_response());
    };

    let did_str = user.did.as_ref().to_string();
    let (handle, avatar) = tokio::join!(
        state.identity.handle_for(&did_str),
        fetch_bsky_avatar(&state.http, &state.config.appview_url, &did_str),
    );
    let handle = handle.unwrap_or(did_str);

    let html = DashboardTemplate {
        handle,
        avatar: avatar.unwrap_or_default(),
    }
    .render()
    .map_err(AppError::internal)?;
    Ok(Html(html).into_response())
}
