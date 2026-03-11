use std::sync::Arc;

use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Form, Json};
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use jacquard::identity::JacquardResolver;
use jacquard::oauth::atproto::{AtprotoClientMetadata, GrantType};
use jacquard::oauth::authstore::MemoryAuthStore;
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::scopes::{Scope, TransitionScope};
use jacquard::oauth::session::ClientData;
use jacquard::oauth::types::{AuthorizeOptions, CallbackParams};
use jacquard_common::types::did::Did;
use serde::Deserialize;
use url::Url;

use crate::error;
use crate::AppState;

/// Concrete OAuth client type used by this application.
pub type OAuthClientType = OAuthClient<JacquardResolver, MemoryAuthStore>;
/// Concrete OAuth session type returned after a successful authorization.
pub type OAuthSessionType =
    jacquard::oauth::client::OAuthSession<JacquardResolver, MemoryAuthStore>;

/// Axum extractor that restores an authenticated OAuth session from the session cookie.
///
/// Use as a handler argument on auth-gated routes:
/// ```ignore
/// pub async fn my_handler(auth: AuthSession, ...) -> Response { ... }
/// ```
/// Returns 401 with a plain-text error if the cookie is missing, malformed, or expired.
pub struct AuthSession(pub OAuthSessionType);

impl FromRequestParts<Arc<AppState>> for AuthSession {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|e| e.into_response())?;

        let (did_str, session_id) = parse_session_cookie(&jar).ok_or_else(|| {
            (
                axum::http::StatusCode::UNAUTHORIZED,
                "Authentication required",
            )
                .into_response()
        })?;

        let did = Did::new(&did_str).map_err(|_| {
            (axum::http::StatusCode::UNAUTHORIZED, "Invalid session").into_response()
        })?;

        let session = state.oauth.restore(&did, &session_id).await.map_err(|e| {
            (
                axum::http::StatusCode::UNAUTHORIZED,
                format!("Session expired: {e}"),
            )
                .into_response()
        })?;

        Ok(AuthSession(session))
    }
}

/// Returns true if the base URL is a loopback address (http://localhost or http://127.0.0.1).
fn is_loopback_base_url(base_url: &str) -> bool {
    base_url.starts_with("http://127.0.0.1") || base_url.starts_with("http://localhost")
}

/// Build the OAuth client for the given base URL.
pub fn build_oauth_client(base_url: &str) -> OAuthClientType {
    // Loopback client_id must be "http://localhost" with scope and redirect_uri
    // encoded as query params — the PDS derives metadata from these params
    // without fetching any URL. Discoverable (production) clients use the full
    // metadata URL with https://.
    let client_id = if is_loopback_base_url(base_url) {
        let scope = urlencoding::encode("atproto transition:generic");
        let redir_raw = format!("{base_url}/oauth/callback");
        let redir = urlencoding::encode(&redir_raw);
        Url::parse(&format!(
            "http://localhost?scope={scope}&redirect_uri={redir}"
        ))
        .unwrap()
    } else {
        Url::parse(&format!(
            "{base_url}/.well-known/oauth-client-metadata.json"
        ))
        .unwrap()
    };
    let redirect_uri = Url::parse(&format!("{base_url}/oauth/callback")).unwrap();
    let client_uri = Url::parse(base_url).unwrap();

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
pub async fn client_metadata(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let base = &state.config.base_url;
    let client_id = if is_loopback_base_url(base) {
        let scope = urlencoding::encode("atproto transition:generic");
        let redir_raw = format!("{base}/oauth/callback");
        let redir = urlencoding::encode(&redir_raw);
        format!("http://localhost?scope={scope}&redirect_uri={redir}")
    } else {
        format!("{base}/.well-known/oauth-client-metadata.json")
    };
    Json(serde_json::json!({
        "client_id": client_id,
        "client_name": "atpr.to URL Shortener",
        "client_uri": base,
        "redirect_uris": [format!("{base}/oauth/callback")],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
        "scope": "atproto transition:generic",
        "application_type": "web",
        "dpop_bound_access_tokens": true
    }))
}

/// Request body for `POST /login`.
#[derive(Deserialize)]
pub struct LoginRequest {
    /// The user's AT Protocol handle (e.g. `alice.bsky.social`).
    pub handle: String,
}

/// Start OAuth login flow. User submits their handle.
#[tracing::instrument(skip_all)]
// coverage:excl-start
pub async fn login(State(state): State<Arc<AppState>>, Form(body): Form<LoginRequest>) -> Response {
    let options = AuthorizeOptions::default();
    tracing::debug!("login: handle={}", body.handle);
    match state.oauth.start_auth(&body.handle, options).await {
        Ok(auth_url) => Redirect::to(&auth_url).into_response(),
        Err(e) => error::bad_request(&format!("Failed to start auth: {e:#?}")),
    }
}
// coverage:excl-stop

/// Query parameters received on the OAuth callback redirect.
#[derive(Deserialize)]
pub struct OAuthCallbackQuery {
    /// Authorization code from the authorization server.
    pub code: String,
    /// State parameter echoed back from the authorization server.
    pub state: Option<String>,
    /// Issuer identifier, used for PAR/DPoP validation.
    pub iss: Option<String>,
}

/// Handle OAuth callback after user authorizes.
// coverage:excl-start
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
            let secure = !is_loopback_base_url(&state.config.base_url);
            let cookie = Cookie::build(("session", cookie_value))
                .path("/")
                .http_only(true)
                .secure(secure)
                .same_site(SameSite::Lax)
                .max_age(time::Duration::days(30));

            let jar = jar.add(cookie);
            (jar, Redirect::temporary("/")).into_response()
        }
        Err(e) => error::internal_error(&format!("OAuth callback failed: {e}")),
    }
}
// coverage:excl-stop

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
    use std::sync::Arc;

    use axum::extract::State;
    use axum_extra::extract::cookie::CookieJar;

    use super::*;
    use crate::{config::Config, AppState};

    #[test]
    fn test_parse_session_cookie_valid() {
        let jar = CookieJar::new().add(Cookie::new("session", "did:plc:abc123|sess-id-456"));
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
        let _client = build_oauth_client("https://atpr.to");
        // Just verify it doesn't panic during construction
    }

    #[tokio::test]
    async fn test_client_metadata_fields() {
        let config = Config::default();
        let state = Arc::new(AppState {
            oauth: build_oauth_client(&config.base_url),
            http: reqwest::Client::new(),
            config,
        });
        let result = client_metadata(State(state)).await;
        let json = &result.0;
        assert!(json["client_id"]
            .as_str()
            .unwrap()
            .contains("/.well-known/"));
        assert!(json["redirect_uris"].is_array());
        assert_eq!(json["dpop_bound_access_tokens"], true);
        assert_eq!(json["client_name"], "atpr.to URL Shortener");
    }
}
