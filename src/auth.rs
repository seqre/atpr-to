use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use jacquard::oauth::atproto::{AtprotoClientMetadata, GrantType};
use jacquard::oauth::authstore::MemoryAuthStore;
use jacquard::oauth::client::OAuthClient;
use jacquard::identity::JacquardResolver;
use jacquard::oauth::scopes::{Scope, TransitionScope};
use jacquard::oauth::session::ClientData;
use jacquard::oauth::types::{AuthorizeOptions, CallbackParams};
use serde::Deserialize;
use url::Url;

use crate::AppState;

pub type OAuthClientType = OAuthClient<JacquardResolver, MemoryAuthStore>;

/// Build the OAuth client for atpr.to.
pub fn build_oauth_client() -> OAuthClientType {
    let client_id =
        Url::parse("https://atpr.to/.well-known/oauth-client-metadata.json").unwrap();
    let redirect_uri = Url::parse("https://atpr.to/oauth/callback").unwrap();
    let client_uri = Url::parse("https://atpr.to").unwrap();

    let config = AtprotoClientMetadata::new(
        client_id,
        Some(client_uri),
        vec![redirect_uri],
        vec![GrantType::AuthorizationCode, GrantType::RefreshToken],
        vec![Scope::Atproto, Scope::Transition(TransitionScope::Generic)],
        None, // jwks_uri — keyset: None auto-generates ES256
    )
    .with_prod_info("atpr.to URL Shortener", None, None, None);

    let client_data = ClientData {
        keyset: None,
        config,
    };

    OAuthClient::new(MemoryAuthStore::new(), client_data)
}

/// Serve OAuth client metadata for atproto OAuth discovery.
pub async fn client_metadata() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "client_id": "https://atpr.to/.well-known/oauth-client-metadata.json",
        "client_name": "atpr.to URL Shortener",
        "client_uri": "https://atpr.to",
        "redirect_uris": ["https://atpr.to/oauth/callback"],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
        "scope": "atproto transition:generic",
        "application_type": "web",
        "dpop_bound_access_tokens": true
    }))
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub handle: String,
}

/// Start OAuth login flow. User submits their handle.
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginRequest>,
) -> Response {
    let options = AuthorizeOptions::default();

    match state.oauth.start_auth(&body.handle, options).await {
        Ok(auth_url) => Redirect::temporary(&auth_url).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            format!("Failed to start auth: {e}"),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: String,
    pub state: Option<String>,
    pub iss: Option<String>,
}

/// Handle OAuth callback after user authorizes.
pub async fn oauth_callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<OAuthCallbackQuery>,
    jar: CookieJar,
) -> Response {
    let params = CallbackParams {
        code: query.code.into(),
        state: query.state.map(Into::into),
        iss: query.iss.map(Into::into),
    };

    match state.oauth.callback(params).await {
        Ok(session) => {
            let (did, session_id) = session.session_info().await;

            let cookie_value = format!("{}|{}", did.as_ref(), session_id.as_ref());
            let cookie = Cookie::build(("session", cookie_value))
                .path("/")
                .http_only(true)
                .secure(true)
                .same_site(SameSite::Lax)
                .max_age(time::Duration::days(30));

            let jar = jar.add(cookie);
            (jar, Redirect::temporary("/")).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("OAuth callback failed: {e}"),
        )
            .into_response(),
    }
}

/// Extract DID and session_id from the session cookie.
/// Returns None if no valid session cookie exists.
pub fn parse_session_cookie(jar: &CookieJar) -> Option<(String, String)> {
    let cookie = jar.get("session")?;
    let value = cookie.value();
    let (did, session_id) = value.split_once('|')?;
    Some((did.to_string(), session_id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_extra::extract::cookie::CookieJar;

    #[test]
    fn test_parse_session_cookie_valid() {
        let jar = CookieJar::new().add(Cookie::new(
            "session",
            "did:plc:abc123|sess-id-456",
        ));
        let result = parse_session_cookie(&jar);
        let (did, session_id) = result.unwrap();
        assert_eq!(did, "did:plc:abc123");
        assert_eq!(session_id, "sess-id-456");
    }

    #[test]
    fn test_parse_session_cookie_missing() {
        let jar = CookieJar::new();
        assert!(parse_session_cookie(&jar).is_none());
    }

    #[test]
    fn test_parse_session_cookie_malformed() {
        let jar = CookieJar::new().add(Cookie::new("session", "no-separator-here"));
        assert!(parse_session_cookie(&jar).is_none());
    }

    #[test]
    fn test_build_oauth_client() {
        let _client = build_oauth_client();
        // Just verify it doesn't panic during construction
    }
}
