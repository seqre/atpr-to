//! A DynamoDB-backed OAuth session store.
//!
//! The fix for the one known-broken thing in this application. On Lambda the
//! other two stores are per *execution environment*: an authorization request
//! written while a login starts is missing when the callback lands on a
//! different instance, so logins fail nondeterministically under any
//! concurrency at all. Reserved concurrency of 1 avoids it by refusing to scale.
//!
//! Every instance shares one table here, so it does not matter which one
//! answers.
//!
//! **Shape.** One table, one string partition key `pk`, holding both kinds of
//! record behind their own prefixes — `session|{did}|{session_id}` and
//! `authreq|{state}`. Two tables would double the deployment surface to
//! separate two things that are written and read on the same paths and never
//! queried together.
//!
//! The payload is stored as one opaque JSON string in `data`, exactly as
//! `file.rs` does. Modelling jacquard's session types as DynamoDB attributes
//! would buy nothing — nothing queries inside them — and would turn every
//! upstream field change into a migration.
//!
//! **Expiry** is DynamoDB's, via a TTL on `expires_at`. `file.rs` had to sweep
//! abandoned authorization requests itself because nothing else would; here the
//! table does it. TTL deletion is asynchronous and documented as "within a few
//! days", so reads filter on the same timestamp rather than trusting the sweep
//! — the same rule `file.rs` follows, for the same reason.

use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use jacquard::oauth::authstore::ClientAuthStore;
use jacquard::oauth::session::{AuthRequestData, ClientSessionData};
use jacquard_common::bos::BosStr;
use jacquard_common::session::SessionStoreError;
use jacquard_common::types::did::Did;

use super::file::AUTH_REQUEST_TTL;

/// Partition key attribute.
const PK: &str = "pk";
/// Opaque JSON payload attribute.
const DATA: &str = "data";
/// Unix seconds after which DynamoDB may delete the item.
const EXPIRES_AT: &str = "expires_at";

/// How long a session record lives without being written again.
///
/// Longer than the session cookie's own 30 days, so the store never expires a
/// session the browser still considers current; a signed-in visitor rewrites
/// their record long before this. It exists so that abandoned rows leave on
/// their own rather than accumulating for the life of the table.
const SESSION_TTL_SECS: u64 = 90 * 24 * 60 * 60;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn session_key(did: &str, session_id: &str) -> String {
    format!("session|{did}|{session_id}")
}

fn auth_key(state: &str) -> String {
    format!("authreq|{state}")
}

/// A DynamoDB-backed [`ClientAuthStore`].
#[derive(Debug, Clone)]
pub struct DynamoStore {
    client: Client,
    table: String,
}

impl DynamoStore {
    /// Connect to `table` using the ambient AWS configuration.
    ///
    /// Region and credentials come from the environment, which on Lambda means
    /// the function's own role and region — nothing to configure, and nothing
    /// secret in `Config.toml`.
    ///
    /// The HTTPS client is built here rather than left to the SDK's default:
    /// the default resolves to rustls with the aws-lc-rs provider, which builds
    /// `aws-lc-sys` from C. That is a cross-compilation hazard for the arm64
    /// Lambda target and a second crypto backend beside the ring one reqwest
    /// already links. See the dependency comment in `Cargo.toml`.
    pub async fn connect(table: impl Into<String>) -> Self {
        let https = aws_smithy_http_client::Builder::new()
            .tls_provider(aws_smithy_http_client::tls::Provider::Rustls(
                aws_smithy_http_client::tls::rustls_provider::CryptoMode::Ring,
            ))
            .build_https();

        let config = aws_config::from_env().http_client(https).load().await;

        Self {
            client: Client::new(&config),
            table: table.into(),
        }
    }

    /// Build from an already-configured client, for tests.
    #[cfg(test)]
    pub fn with_client(client: Client, table: impl Into<String>) -> Self {
        Self {
            client,
            table: table.into(),
        }
    }

    /// Read one item's payload, if it is present and not past its expiry.
    async fn get(&self, key: String) -> Result<Option<String>, SessionStoreError> {
        let out = self
            .client
            .get_item()
            .table_name(&self.table)
            .key(PK, AttributeValue::S(key))
            .send()
            .await
            .map_err(Self::upstream)?;

        let Some(item) = out.item else {
            return Ok(None);
        };

        // TTL deletion is asynchronous, so an expired item can still be read
        // back. Filtering here is what makes the deadline real.
        if let Some(AttributeValue::N(expires)) = item.get(EXPIRES_AT) {
            if expires.parse::<u64>().is_ok_and(|e| e <= now_secs()) {
                return Ok(None);
            }
        }

        match item.get(DATA) {
            Some(AttributeValue::S(json)) => Ok(Some(json.clone())),
            _ => Err(SessionStoreError::Other(
                "stored item has no readable payload".into(),
            )),
        }
    }

    /// Write one item with a payload and a deadline.
    async fn put(&self, key: String, json: String, ttl: u64) -> Result<(), SessionStoreError> {
        self.client
            .put_item()
            .table_name(&self.table)
            .item(PK, AttributeValue::S(key))
            .item(DATA, AttributeValue::S(json))
            .item(
                EXPIRES_AT,
                AttributeValue::N((now_secs() + ttl).to_string()),
            )
            .send()
            .await
            .map_err(Self::upstream)?;
        Ok(())
    }

    async fn remove(&self, key: String) -> Result<(), SessionStoreError> {
        self.client
            .delete_item()
            .table_name(&self.table)
            .key(PK, AttributeValue::S(key))
            .send()
            .await
            .map_err(Self::upstream)?;
        Ok(())
    }

    /// Collapse an SDK error into the trait's error type.
    ///
    /// The SDK's Display is one line and hides the cause, so the chain is
    /// walked here — this text goes to the log, never to a client.
    fn upstream<E, R>(e: aws_sdk_dynamodb::error::SdkError<E, R>) -> SessionStoreError
    where
        E: std::error::Error + 'static,
        R: std::fmt::Debug,
    {
        let mut chain = e.to_string();
        let mut source = std::error::Error::source(&e);
        while let Some(cause) = source {
            chain.push_str(": ");
            chain.push_str(&cause.to_string());
            source = cause.source();
        }
        SessionStoreError::Other(format!("dynamodb: {chain}").into())
    }
}

impl ClientAuthStore for DynamoStore {
    async fn get_session<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<Option<ClientSessionData>, SessionStoreError> {
        let Some(json) = self.get(session_key(did.as_ref(), session_id)).await? else {
            return Ok(None);
        };
        serde_json::from_str(&json).map(Some).map_err(|e| {
            SessionStoreError::Other(format!("stored session is unreadable: {e}").into())
        })
    }

    async fn upsert_session(&self, session: ClientSessionData) -> Result<(), SessionStoreError> {
        let key = session_key(session.account_did.as_ref(), session.session_id.as_ref());
        let json = serde_json::to_string(&session).map_err(|e| {
            SessionStoreError::Other(format!("session is unserializable: {e}").into())
        })?;
        self.put(key, json, SESSION_TTL_SECS).await
    }

    async fn delete_session<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<(), SessionStoreError> {
        self.remove(session_key(did.as_ref(), session_id)).await
    }

    async fn get_auth_req_info(
        &self,
        state_param: &str,
    ) -> Result<Option<AuthRequestData>, SessionStoreError> {
        let Some(json) = self.get(auth_key(state_param)).await? else {
            return Ok(None);
        };
        serde_json::from_str(&json).map(Some).map_err(|e| {
            SessionStoreError::Other(format!("stored auth request is unreadable: {e}").into())
        })
    }

    async fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData,
    ) -> Result<(), SessionStoreError> {
        let key = auth_key(AsRef::<str>::as_ref(&auth_req_info.state));
        let json = serde_json::to_string(auth_req_info).map_err(|e| {
            SessionStoreError::Other(format!("auth request is unserializable: {e}").into())
        })?;
        self.put(key, json, AUTH_REQUEST_TTL.as_secs()).await
    }

    async fn delete_auth_req_info(&self, state_param: &str) -> Result<(), SessionStoreError> {
        self.remove(auth_key(state_param)).await
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use aws_sdk_dynamodb::config::{BehaviorVersion, Credentials, Region};
    use aws_smithy_http_client::test_util::{ReplayEvent, StaticReplayClient};
    use aws_smithy_types::body::SdkBody;

    /// A client that answers each request in turn from `bodies`, and remembers
    /// what it was asked. No AWS, no network, no credentials that mean anything.
    fn store_replaying_all(bodies: &[&str]) -> (DynamoStore, StaticReplayClient) {
        let events = bodies
            .iter()
            .map(|body| {
                ReplayEvent::new(
                    http::Request::builder()
                        .uri("https://dynamodb.us-east-1.amazonaws.com/")
                        .body(SdkBody::empty())
                        .unwrap(),
                    http::Response::builder()
                        .status(200)
                        .body(SdkBody::from(body.to_string()))
                        .unwrap(),
                )
            })
            .collect();
        let http = StaticReplayClient::new(events);

        let conf = aws_sdk_dynamodb::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new("ak", "sk", None, None, "test"))
            .http_client(http.clone())
            .build();

        (
            DynamoStore::with_client(Client::from_conf(conf), "sessions"),
            http,
        )
    }

    fn store_replaying(body: &str) -> (DynamoStore, StaticReplayClient) {
        store_replaying_all(&[body])
    }

    /// A store that answers `n` requests with an empty item, for the shared
    /// suite in `super::tests`.
    pub(crate) fn empty_store(n: usize) -> DynamoStore {
        store_replaying_all(&vec!["{}"; n]).0
    }

    /// Sessions and authorization requests share a table, so their keys must
    /// not be able to collide — a state parameter that happened to look like a
    /// DID would otherwise read back somebody's session.
    #[test]
    fn test_the_two_kinds_of_key_cannot_collide() {
        assert!(session_key("did:plc:abc", "s1").starts_with("session|"));
        assert!(auth_key("state-1").starts_with("authreq|"));
        assert_ne!(
            session_key("did:plc:abc", "s1"),
            auth_key("did:plc:abc|s1"),
            "prefixes are what keep these apart"
        );
    }

    #[tokio::test]
    async fn test_a_missing_item_is_none_not_an_error() {
        let (store, _http) = store_replaying("{}");
        assert!(store
            .get("session|nobody".to_string())
            .await
            .unwrap()
            .is_none());
    }

    /// TTL deletion is asynchronous and AWS documents it as happening within
    /// days, so an expired item can still come back over the wire. The deadline
    /// is only real because the read enforces it too.
    #[tokio::test]
    async fn test_an_expired_item_reads_as_absent() {
        let past = now_secs() - 1;
        let body = format!(
            r#"{{"Item":{{"pk":{{"S":"authreq|s"}},"data":{{"S":"{{}}"}},"expires_at":{{"N":"{past}"}}}}}}"#
        );
        let (store, _http) = store_replaying(&body);
        assert!(store.get("authreq|s".to_string()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_a_live_item_returns_its_payload() {
        let future = now_secs() + 600;
        let body = format!(
            r#"{{"Item":{{"pk":{{"S":"authreq|s"}},"data":{{"S":"{{\"hello\":1}}"}},"expires_at":{{"N":"{future}"}}}}}}"#
        );
        let (store, _http) = store_replaying(&body);
        assert_eq!(
            store.get("authreq|s".to_string()).await.unwrap().as_deref(),
            Some(r#"{"hello":1}"#)
        );
    }

    /// An item without the payload attribute is a bug on our side or a foreign
    /// writer, and either way is not "no session" — answering None would sign
    /// somebody out and hide the cause.
    #[tokio::test]
    async fn test_an_item_without_a_payload_is_an_error() {
        let (store, _http) = store_replaying(r#"{"Item":{"pk":{"S":"authreq|s"}}}"#);
        assert!(store.get("authreq|s".to_string()).await.is_err());
    }

    /// The write must carry a deadline, or the table grows forever.
    #[tokio::test]
    async fn test_a_write_sets_an_expiry_in_the_future() {
        let (store, http) = store_replaying("{}");
        store
            .put("authreq|s".to_string(), "{}".to_string(), 600)
            .await
            .unwrap();

        let sent = http.actual_requests().next().expect("a request was made");
        let body = std::str::from_utf8(sent.body().bytes().unwrap()).unwrap();
        let json: serde_json::Value = serde_json::from_str(body).unwrap();
        let expires: u64 = json["Item"]["expires_at"]["N"]
            .as_str()
            .expect("expires_at is written as a number attribute")
            .parse()
            .unwrap();
        assert!(expires > now_secs(), "the deadline must be ahead of now");
    }
}
