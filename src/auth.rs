use std::future::Future;
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
use jacquard::oauth::scopes::Scopes;
use jacquard::oauth::session::ClientData;
use jacquard::oauth::types::{AuthorizeOptions, CallbackParams};
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::deps::smol_str::SmolStr;
use jacquard_common::types::did::Did;
use serde::Deserialize;

use crate::config::{BaseUrl, SessionStore};
use crate::error::AppError;
use crate::session::{AuthStore, FileStore};
use crate::store::{LinkStore, PdsLinkStore};
use crate::AppState;

/// The identity resolver this application uses, over the shared `reqwest` client.
pub type Resolver = JacquardResolver<reqwest::Client>;
/// Concrete OAuth client type used by this application.
pub type OAuthClientType = OAuthClient<Resolver, AuthStore>;
/// Concrete OAuth session type returned after a successful authorization.
pub type OAuthSessionType = jacquard::oauth::client::OAuthSession<Resolver, AuthStore>;

/// An authenticated caller and the store their links live in.
///
/// The DID is parsed once, here at the edge. It used to be re-parsed with
/// `Did::new_owned` inside `shorten`, `delete_link` and `list_links`, each with
/// its own "Invalid DID in session" branch for a case that cannot happen once
/// the cookie has been validated.
///
/// Generic over the [`Authenticator`] rather than over the store type directly:
/// `AuthedUser<A::Store>` cannot work, because an associated-type projection in
/// the self position leaves `A` unconstrained (E0207).
pub struct AuthedUser<A: Authenticator> {
    /// The caller's DID.
    pub did: Did,
    /// Their link store.
    pub store: A::Store,
}

/// Query parameters handed back on the OAuth callback.
///
/// Our own type rather than jacquard's `CallbackParams`, so the port does not
/// leak the protocol library into `api/`.
#[derive(Debug, Clone, Deserialize)]
pub struct CallbackInput {
    /// Authorization code from the authorization server.
    pub code: String,
    /// State parameter echoed back from the authorization server.
    pub state: Option<String>,
    /// Issuer identifier, used for PAR/DPoP validation.
    pub iss: Option<String>,
}

/// The whole authentication lifecycle: start a login, finish one, and turn a
/// cookie into an authenticated user.
///
/// This is the seam that lets the authed paths be tested without a live PDS.
/// Production wires [`OAuthAuthenticator`] (`Store = PdsLinkStore`); tests wire
/// [`FakeAuthenticator`] (`Store = InMemoryLinkStore`). `router_with_state`
/// monomorphises over it, so there is no boxing and no dynamic dispatch.
///
/// Login and callback are part of the trait rather than reaching into a
/// concrete OAuth client, so the callback route is exercisable too — it was
/// previously `coverage:excl` for exactly the reason that it was not.
pub trait Authenticator: Send + Sync + 'static {
    /// The link store this authenticator hands out.
    type Store: LinkStore;

    /// Authenticate a request, or explain why not.
    fn authenticate(
        &self,
        jar: &CookieJar,
    ) -> impl Future<Output = Result<AuthedUser<Self>, AppError>> + Send
    where
        Self: Sized;

    /// Begin a login for `handle`, returning the URL to send the user to.
    fn start_login(&self, handle: &str) -> impl Future<Output = Result<String, AppError>> + Send;

    /// Complete a login, returning the value for the session cookie.
    fn complete_login(
        &self,
        input: CallbackInput,
    ) -> impl Future<Output = Result<String, AppError>> + Send;

    /// Revoke the session this cookie names.
    ///
    /// Clearing the cookie is not logging out: the access token, refresh token
    /// and DPoP key stayed live in the store forever, so a cookie captured
    /// before "sign out" kept working afterwards. Best-effort — a caller cannot
    /// do anything useful with a revocation failure, and must still be logged
    /// out locally either way.
    fn revoke(&self, jar: &CookieJar) -> impl Future<Output = ()> + Send;
}

impl<A: Authenticator> FromRequestParts<Arc<AppState<A>>> for AuthedUser<A> {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState<A>>,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::Unauthorized)?;
        state.auth.authenticate(&jar).await
    }
}

/// An authenticated user, if there is one.
///
/// For routes that render differently when signed in but do not require it.
/// `ui::home` used to decide this by checking whether a `session` cookie was
/// *present*, which any client can arrange, so an unauthenticated visitor
/// holding a junk cookie was bounced to `/dashboard` and straight back.
pub struct MaybeAuth<A: Authenticator>(pub Option<AuthedUser<A>>);

impl<A: Authenticator> MaybeAuth<A> {
    /// Whether the request carried a valid session.
    pub fn is_authenticated(&self) -> bool {
        self.0.is_some()
    }
}

impl<A: Authenticator> FromRequestParts<Arc<AppState<A>>> for MaybeAuth<A> {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState<A>>,
    ) -> Result<Self, Self::Rejection> {
        let Ok(jar) = CookieJar::from_request_parts(parts, state).await;
        Ok(MaybeAuth(state.auth.authenticate(&jar).await.ok()))
    }
}

/// The production [`Authenticator`]: restores an OAuth session from the cookie.
pub struct OAuthAuthenticator {
    /// The OAuth client used to restore sessions.
    pub oauth: OAuthClientType,
}

impl OAuthAuthenticator {
    /// Wrap an OAuth client as an authenticator.
    pub fn new(oauth: OAuthClientType) -> Self {
        Self { oauth }
    }

    /// Restore a session without producing a store, for callers that only need
    /// to know whether the cookie is still good.
    pub async fn restore(&self, jar: &CookieJar) -> Result<(Did, OAuthSessionType), AppError> {
        let (did_str, session_id) = parse_session_cookie(jar).ok_or(AppError::Unauthorized)?;
        let did: Did = Did::new_owned(&did_str).map_err(|_| AppError::Unauthorized)?;

        // The reason a session failed to restore is not the caller's business —
        // it previously came back as `Session expired: {e}`, echoing jacquard's
        // internal error text to anyone who sent a cookie.
        let session = self.oauth.restore(&did, &session_id).await.map_err(|e| {
            tracing::debug!(err = %e, "session restore failed");
            AppError::Unauthorized
        })?;

        Ok((did, session))
    }
}

impl Authenticator for OAuthAuthenticator {
    type Store = PdsLinkStore;

    async fn authenticate(&self, jar: &CookieJar) -> Result<AuthedUser<Self>, AppError> {
        let (did, session) = self.restore(jar).await?;
        Ok(AuthedUser {
            store: PdsLinkStore::new(session, did.clone()),
            did,
        })
    }

    async fn start_login(&self, handle: &str) -> Result<String, AppError> {
        self.oauth
            .start_auth(handle, AuthorizeOptions::<SmolStr>::default())
            .await
            .map_err(|e| AppError::Upstream(anyhow::anyhow!("{e:#?}")))
    }

    async fn complete_login(&self, input: CallbackInput) -> Result<String, AppError> {
        let params: CallbackParams<SmolStr> = CallbackParams {
            code: SmolStr::from(input.code),
            state: input.state.map(SmolStr::from),
            iss: input.iss.map(SmolStr::from),
        };

        let session = self
            .oauth
            .callback(params)
            .await
            .map_err(|e| AppError::Upstream(anyhow::anyhow!("OAuth callback failed: {e}")))?;

        let (did, session_id) = session.session_info().await;
        Ok(format!("{}|{}", did.as_ref(), session_id.as_str()))
    }

    async fn revoke(&self, jar: &CookieJar) {
        let Ok((did, session)) = self.restore(jar).await else {
            // No live session to revoke; clearing the cookie is all there is.
            return;
        };

        // `OAuthSession::logout` calls the authorization server's revocation
        // endpoint and then drops the session from the store.
        if let Err(e) = session.logout().await {
            tracing::warn!(did = %did.as_ref(), err = %e, "server-side session revocation failed");
        } else {
            tracing::info!(did = %did.as_ref(), "session revoked");
        }
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
pub async fn build_oauth_client(
    base_url: &BaseUrl,
    session_store: &SessionStore,
    http: reqwest::Client,
) -> Result<OAuthClientType, std::io::Error> {
    let client_data = ClientData {
        keyset: None,
        config: client_metadata_for(base_url),
    };

    let store = match session_store.path() {
        None => AuthStore::Memory(MemoryAuthStore::new()),
        Some(path) => AuthStore::File(FileStore::open(path).await?),
    };
    Ok(OAuthClient::new(store, client_data, http))
}

/// Serve OAuth client metadata for atproto OAuth discovery.
///
/// Built from the same `client_id` / `redirect_uri` / `SCOPE` helpers the client
/// itself registers with, so the published document cannot describe a client
/// different from the one running.
pub async fn client_metadata<A: Authenticator>(
    State(state): State<Arc<AppState<A>>>,
) -> Json<serde_json::Value> {
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
pub async fn login<A: Authenticator>(
    State(state): State<Arc<AppState<A>>>,
    Form(body): Form<LoginRequest>,
) -> Result<Redirect, AppError> {
    tracing::debug!(handle = %body.handle, "starting login");
    let auth_url = state.auth.start_login(&body.handle).await?;
    Ok(Redirect::to(&auth_url))
}

/// Handle OAuth callback after the user authorizes.
pub async fn oauth_callback<A: Authenticator>(
    State(state): State<Arc<AppState<A>>>,
    Query(query): Query<CallbackInput>,
    jar: CookieJar,
) -> Result<Response, AppError> {
    let cookie_value = state.auth.complete_login(query).await?;
    let cookie = session_cookie(&state.config.base_url, cookie_value);
    Ok((jar.add(cookie), Redirect::temporary("/")).into_response())
}

/// How long a session cookie lives.
const SESSION_MAX_AGE: time::Duration = time::Duration::days(30);

/// The session cookie's name.
///
/// The `__Host-` prefix is free hardening: browsers refuse to accept such a
/// cookie unless it is `Secure`, `Path=/` and carries no `Domain`, which means
/// a subdomain cannot set or overwrite it.
///
/// It requires `Secure`, so loopback keeps the plain name. `http://127.0.0.1`
/// is a trustworthy origin and most current browsers would accept a `Secure`
/// cookie there, but "most current browsers" is not a good foundation for the
/// local development flow.
pub fn cookie_name(base_url: &BaseUrl) -> &'static str {
    if base_url.is_loopback() {
        "session"
    } else {
        "__Host-session"
    }
}

/// Build the session cookie carrying `value`.
pub fn session_cookie(base_url: &BaseUrl, value: String) -> Cookie<'static> {
    Cookie::build((cookie_name(base_url), value))
        .path("/")
        .http_only(true)
        .secure(!base_url.is_loopback())
        .same_site(SameSite::Lax)
        .max_age(SESSION_MAX_AGE)
        .build()
}

/// Clear the session cookie.
///
/// Adds an already-expired cookie rather than calling `CookieJar::remove`,
/// because `remove` only emits a `Set-Cookie` when the jar *already held* a
/// cookie of that name — so a client holding the other name would silently keep
/// its session after "sign out". Both names are cleared for the same reason.
///
/// The attributes mirror the cookie being cleared: a browser matches a removal
/// on name, path and domain, so a clearing cookie that disagrees about `Path`
/// clears nothing.
pub fn clear_session(jar: CookieJar, base_url: &BaseUrl) -> CookieJar {
    let mut jar = jar;
    for name in ["__Host-session", "session"] {
        // `__Host-` requires Secure; the plain name mirrors whatever the
        // configured base URL implies.
        let secure = name.starts_with("__Host-") || !base_url.is_loopback();
        jar = jar.add(
            Cookie::build((name, ""))
                .path("/")
                .http_only(true)
                .secure(secure)
                .same_site(SameSite::Lax)
                .max_age(time::Duration::seconds(0))
                .build(),
        );
    }
    jar
}

/// The raw session cookie value, under either name.
///
/// Accepts both so a deployment that moves between loopback and production — or
/// a browser still holding the pre-`__Host-` cookie — does not wedge.
pub fn session_cookie_value(jar: &CookieJar) -> Option<&str> {
    jar.get("__Host-session")
        .or_else(|| jar.get("session"))
        .map(|c| c.value())
}

/// Extract DID and session_id from the session cookie.
///
/// Returns None if no valid session cookie exists.
pub fn parse_session_cookie(jar: &CookieJar) -> Option<(String, String)> {
    let (did, session_id) = session_cookie_value(jar)?.split_once('|')?;
    Some((did.to_string(), session_id.to_string()))
}

/// An [`Authenticator`] that accepts a fixed cookie value and hands out an
/// in-memory store.
///
/// Not behind `#[cfg(test)]`: the integration tests in `tests/` are a separate
/// crate and need it. This is what makes the whole authed write path testable
/// without a live PDS — the reason `shorten`, `delete_link` and `list_links`
/// were previously annotated out of coverage rather than covered.
pub struct FakeAuthenticator {
    /// The DID handed to every authenticated caller.
    pub did: Did,
    /// The cookie value that authenticates. Any other value is rejected.
    pub accepts_cookie: String,
    /// The store handed to every authenticated caller.
    pub store: Arc<crate::store::InMemoryLinkStore>,
    /// When set, `start_login` and `complete_login` fail with this message.
    pub login_fails: Option<String>,
    /// Set by `revoke`, so a test can assert the session was actually revoked
    /// server-side and not merely un-cookied.
    pub revoked: Arc<std::sync::atomic::AtomicBool>,
}

impl FakeAuthenticator {
    /// An authenticator accepting `session=<did>|test-session`.
    pub fn new(did: &str) -> Self {
        Self {
            did: Did::new_owned(did).expect("test DID is valid"),
            accepts_cookie: format!("{did}|test-session"),
            store: Arc::new(crate::store::InMemoryLinkStore::new()),
            login_fails: None,
            revoked: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Whether `revoke` has been called with a matching cookie.
    pub fn was_revoked(&self) -> bool {
        self.revoked.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The cookie header value a client should send to authenticate.
    pub fn cookie_header(&self) -> String {
        format!("session={}", self.accepts_cookie)
    }

    /// Replace the backing store, e.g. with one that always fails.
    pub fn with_store(mut self, store: crate::store::InMemoryLinkStore) -> Self {
        self.store = Arc::new(store);
        self
    }

    /// Make the login endpoints fail.
    pub fn with_failing_login(mut self, message: impl Into<String>) -> Self {
        self.login_fails = Some(message.into());
        self
    }
}

impl Authenticator for FakeAuthenticator {
    type Store = Arc<crate::store::InMemoryLinkStore>;

    async fn authenticate(&self, jar: &CookieJar) -> Result<AuthedUser<Self>, AppError> {
        if session_cookie_value(jar) != Some(self.accepts_cookie.as_str()) {
            return Err(AppError::Unauthorized);
        }
        Ok(AuthedUser {
            did: self.did.clone(),
            store: Arc::clone(&self.store),
        })
    }

    async fn start_login(&self, handle: &str) -> Result<String, AppError> {
        if let Some(m) = &self.login_fails {
            return Err(AppError::Upstream(anyhow::anyhow!("{m}")));
        }
        Ok(format!(
            "https://pds.test/oauth/authorize?handle={}",
            urlencoding::encode(handle)
        ))
    }

    async fn complete_login(&self, _input: CallbackInput) -> Result<String, AppError> {
        if let Some(m) = &self.login_fails {
            return Err(AppError::Upstream(anyhow::anyhow!("{m}")));
        }
        Ok(self.accepts_cookie.clone())
    }

    async fn revoke(&self, jar: &CookieJar) {
        if session_cookie_value(jar) == Some(self.accepts_cookie.as_str()) {
            self.revoked
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::extract::State;
    use axum_extra::extract::cookie::CookieJar;

    use super::*;
    use crate::config::Config;

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

    #[tokio::test]
    async fn test_build_oauth_client() {
        let base = BaseUrl::parse("https://atpr.to").unwrap();
        let _client = build_oauth_client(&base, &SessionStore::Memory, reqwest::Client::new())
            .await
            .expect("in-memory store cannot fail");
    }

    #[tokio::test]
    async fn test_client_metadata_fields() {
        let state = crate::build_state(Config::default()).await.unwrap();
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
