use std::sync::Arc;

use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Form, Json};
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use jacquard::client::FileAuthStore;
use jacquard::identity::JacquardResolver;
use jacquard::oauth::atproto::{AtprotoClientMetadata, GrantType};
use jacquard::oauth::authstore::{ClientAuthStore, MemoryAuthStore};
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::scopes::Scopes;
use jacquard::oauth::session::{AuthRequestData, ClientData, ClientSessionData};
use jacquard::oauth::types::{AuthorizeOptions, CallbackParams};
use jacquard_common::bos::BosStr;
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::deps::smol_str::SmolStr;
use jacquard_common::session::SessionStoreError;
use jacquard_common::types::did::Did;
use serde::Deserialize;

use crate::config::{BaseUrl, SessionStore};
use crate::error::AppError;
use crate::AppState;

/// Wraps either a memory-backed or file-backed OAuth session store.
pub enum AuthStore {
    /// In-memory store (sessions lost on restart).
    Memory(MemoryAuthStore),
    /// File-backed store (sessions persist across restarts).
    File(FileAuthStore),
}

impl ClientAuthStore for AuthStore {
    async fn get_session<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<Option<ClientSessionData>, SessionStoreError> {
        match self {
            AuthStore::Memory(s) => s.get_session(did, session_id).await,
            AuthStore::File(s) => s.get_session(did, session_id).await,
        }
    }

    async fn upsert_session(&self, session: ClientSessionData) -> Result<(), SessionStoreError> {
        match self {
            AuthStore::Memory(s) => s.upsert_session(session).await,
            AuthStore::File(s) => s.upsert_session(session).await,
        }
    }

    async fn delete_session<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<(), SessionStoreError> {
        match self {
            AuthStore::Memory(s) => s.delete_session(did, session_id).await,
            AuthStore::File(s) => s.delete_session(did, session_id).await,
        }
    }

    async fn get_auth_req_info(
        &self,
        state: &str,
    ) -> Result<Option<AuthRequestData>, SessionStoreError> {
        match self {
            AuthStore::Memory(s) => s.get_auth_req_info(state).await,
            AuthStore::File(s) => s.get_auth_req_info(state).await,
        }
    }

    async fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData,
    ) -> Result<(), SessionStoreError> {
        match self {
            AuthStore::Memory(s) => s.save_auth_req_info(auth_req_info).await,
            AuthStore::File(s) => s.save_auth_req_info(auth_req_info).await,
        }
    }

    async fn delete_auth_req_info(&self, state: &str) -> Result<(), SessionStoreError> {
        match self {
            AuthStore::Memory(s) => s.delete_auth_req_info(state).await,
            AuthStore::File(s) => s.delete_auth_req_info(state).await,
        }
    }
}

/// The identity resolver this application uses, over the shared `reqwest` client.
pub type Resolver = JacquardResolver<reqwest::Client>;
/// Concrete OAuth client type used by this application.
pub type OAuthClientType = OAuthClient<Resolver, AuthStore>;
/// Concrete OAuth session type returned after a successful authorization.
pub type OAuthSessionType = jacquard::oauth::client::OAuthSession<Resolver, AuthStore>;

/// Axum extractor that restores an authenticated OAuth session from the session cookie.
///
/// Use as a handler argument on auth-gated routes:
/// ```ignore
/// pub async fn my_handler(auth: AuthSession, ...) -> Response { ... }
/// ```
/// Rejects with `AppError::Unauthorized` if the cookie is missing, malformed,
/// or no longer restorable — so the rejection body matches every other error the
/// API emits, rather than being the one `text/plain` response left over.
pub struct AuthSession(pub OAuthSessionType);

impl FromRequestParts<Arc<AppState>> for AuthSession {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::Unauthorized)?;

        let (did_str, session_id) = parse_session_cookie(&jar).ok_or(AppError::Unauthorized)?;
        let did: Did = Did::new_owned(&did_str).map_err(|_| AppError::Unauthorized)?;

        // The reason a session failed to restore is not the caller's business —
        // it previously came back as `Session expired: {e}`, echoing jacquard's
        // internal error text to anyone who sent a cookie.
        let session = state.oauth.restore(&did, &session_id).await.map_err(|e| {
            tracing::debug!(err = %e, "session restore failed");
            AppError::Unauthorized
        })?;

        Ok(AuthSession(session))
    }
}

/// The OAuth scope this client requests.
///
/// Single definition; this literal previously appeared in four places.
pub const SCOPE: &str = "atproto include:to.atpr.fullPermissions";

/// The `client_id` this instance identifies itself with.
///
/// A loopback client_id must be `http://localhost` with the scope and
/// redirect_uri as query params — the PDS derives the metadata from those
/// params without fetching any URL. Discoverable (production) clients use the
/// full https metadata URL.
pub fn client_id(base_url: &BaseUrl) -> String {
    if base_url.is_loopback() {
        let scope = urlencoding::encode(SCOPE);
        let redir = urlencoding::encode_binary(redirect_uri(base_url).as_bytes()).into_owned();
        format!("http://localhost?scope={scope}&redirect_uri={redir}")
    } else {
        format!("{base_url}/oauth-client-metadata.json")
    }
}

/// The OAuth redirect URI for this instance.
pub fn redirect_uri(base_url: &BaseUrl) -> String {
    format!("{base_url}/oauth/callback")
}

/// Build this client's atproto OAuth metadata.
///
/// `build_oauth_client` and `client_metadata` used to construct the same
/// strings independently, so the JSON served at
/// `/oauth-client-metadata.json` could drift from what the client actually
/// registered. Both now go through here.
pub fn client_metadata_for(base_url: &BaseUrl) -> AtprotoClientMetadata<SmolStr> {
    let scopes: Scopes<SmolStr> = Scopes::new(SmolStr::new(SCOPE)).expect("valid scopes");
    AtprotoClientMetadata {
        client_id: Uri::parse(client_id(base_url)).expect("client_id built from a validated URL"),
        client_uri: Some(Uri::parse(base_url.as_str().to_string()).expect("base URL is validated")),
        redirect_uris: vec![
            Uri::parse(redirect_uri(base_url)).expect("redirect URI built from a validated URL")
        ],
        grant_types: vec![GrantType::AuthorizationCode, GrantType::RefreshToken],
        scopes,
        jwks_uri: None,
        client_name: None,
        logo_uri: None,
        tos_uri: None,
        privacy_policy_uri: None,
    }
    .with_prod_info(SmolStr::new("atpr.to URL Shortener"), None, None, None)
}

/// Build the OAuth client.
///
/// `http` is the application's shared `reqwest` client — jacquard 0.12 takes the
/// HTTP client as a constructor argument rather than building its own, so the
/// OAuth and identity paths inherit its timeouts and user-agent.
pub fn build_oauth_client(
    base_url: &BaseUrl,
    session_store: &SessionStore,
    http: reqwest::Client,
) -> OAuthClientType {
    let client_data = ClientData {
        keyset: None,
        config: client_metadata_for(base_url),
    };

    let store = match session_store.path() {
        None => AuthStore::Memory(MemoryAuthStore::new()),
        Some(path) => AuthStore::File(FileAuthStore::new(path)),
    };
    OAuthClient::new(store, client_data, http)
}

/// Serve OAuth client metadata for atproto OAuth discovery.
///
/// Built from the same `client_id` / `redirect_uri` / `SCOPE` helpers the client
/// itself registers with, so the published document cannot describe a client
/// different from the one running.
pub async fn client_metadata(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let base = &state.config.base_url;
    Json(serde_json::json!({
        "client_id": client_id(base),
        "client_name": "atpr.to URL Shortener",
        "client_uri": base.as_str(),
        "redirect_uris": [redirect_uri(base)],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
        "scope": SCOPE,
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
pub async fn login(
    State(state): State<Arc<AppState>>,
    Form(body): Form<LoginRequest>,
) -> Result<Redirect, AppError> {
    let options = AuthorizeOptions::<SmolStr>::default();
    tracing::debug!("login: handle={}", body.handle);
    let auth_url = state
        .oauth
        .start_auth(&body.handle, options)
        .await
        .map_err(|e| AppError::Upstream(anyhow::anyhow!("{e:#?}")))?;
    Ok(Redirect::to(&auth_url))
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
) -> Result<Response, AppError> {
    let params: CallbackParams<SmolStr> = CallbackParams {
        code: SmolStr::from(query.code),
        state: query.state.map(SmolStr::from),
        iss: query.iss.map(SmolStr::from),
    };

    let session = state
        .oauth
        .callback(params)
        .await
        .map_err(|e| AppError::Upstream(anyhow::anyhow!("OAuth callback failed: {e}")))?;

    let (did, session_id) = session.session_info().await;
    let cookie_value = format!("{}|{}", did.as_ref(), session_id.as_str());
    let cookie = Cookie::build(("session", cookie_value))
        .path("/")
        .http_only(true)
        .secure(!state.config.base_url.is_loopback())
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(30));

    Ok((jar.add(cookie), Redirect::temporary("/")).into_response())
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
        let _client = build_oauth_client(
            &BaseUrl::parse("https://atpr.to").unwrap(),
            &SessionStore::Memory,
            reqwest::Client::new(),
        );
        // Just verify it doesn't panic during construction
    }

    #[tokio::test]
    async fn test_client_metadata_fields() {
        let config = Config::default();
        let http = crate::http_client();
        let state = Arc::new(AppState {
            oauth: build_oauth_client(&config.base_url, &config.session_store, http.clone()),
            resolver: crate::identity_resolver(http.clone()),
            http,
            config,
        });
        let result = client_metadata(State(state)).await;
        let json = &result.0;
        assert!(json["client_id"]
            .as_str()
            .unwrap()
            .contains("/oauth-client-metadata.json"));
        assert!(json["redirect_uris"].is_array());
        assert_eq!(json["dpop_bound_access_tokens"], true);
        assert_eq!(json["client_name"], "atpr.to URL Shortener");
    }
}
