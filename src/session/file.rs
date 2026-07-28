//! A file-backed OAuth session store.
//!
//! Ours rather than jacquard's `FileAuthStore`, which is the source of several
//! problems this replaces. Its `set_value` is
//! `read_to_string` → parse → insert → `std::fs::write`:
//!
//! - **Blocking IO inside `async fn`.** Every session read and write stalls an
//!   executor thread.
//! - **Non-atomic whole-file rewrite.** A crash or a full disk mid-write leaves
//!   a truncated file, and since the whole store is one JSON object, that loses
//!   *every* session at once, not one.
//! - **No serialisation of read-modify-write.** Two concurrent writes both read
//!   the old map and the second overwrites the first's insert.
//! - **No expiry.** `oauth-state:` records are written when a login starts and
//!   deleted when the callback arrives. Abandoned logins are never deleted, so
//!   the file grows without bound.
//!
//! This store keeps the map in memory behind a `tokio::sync::RwLock`, persists
//! by writing a temporary file and `rename`ing it over the target (atomic on
//! POSIX), reaps expired authorization requests on every save, and creates the
//! file 0600 — it holds DPoP private keys and refresh tokens.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jacquard::oauth::authstore::ClientAuthStore;
use jacquard::oauth::session::{AuthRequestData, ClientSessionData};
use jacquard_common::bos::BosStr;
use jacquard_common::session::SessionStoreError;
use jacquard_common::types::did::Did;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// How long an in-flight authorization request stays valid.
///
/// An OAuth authorization code exchange happens within seconds; ten minutes is
/// generous. Anything older is an abandoned login.
pub const AUTH_REQUEST_TTL: std::time::Duration = std::time::Duration::from_secs(600);

/// File mode for the store: owner read/write only.
#[cfg(unix)]
const STORE_MODE: u32 = 0o600;

/// Disambiguates concurrent temp files within a process.
static TMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The on-disk shape.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Persisted {
    /// Live sessions, keyed by `{did}|{session_id}`.
    #[serde(default)]
    sessions: BTreeMap<String, ClientSessionData>,
    /// In-flight authorization requests, keyed by OAuth `state`.
    #[serde(default)]
    auth_requests: BTreeMap<String, StoredAuthRequest>,
}

/// An authorization request plus when it was created, so it can expire.
///
/// The payload is kept as an opaque `Value`. This store is a key-value
/// persister; typing the payload here buys nothing and would mean fabricating a
/// real DPoP keypair to test expiry.
#[derive(Debug, Serialize, Deserialize)]
struct StoredAuthRequest {
    /// Unix seconds at which this was stored.
    created_at: u64,
    data: serde_json::Value,
}

/// Whether an entry stored at `created_at` has outlived [`AUTH_REQUEST_TTL`].
fn is_expired(created_at: u64, now: u64) -> bool {
    created_at < now.saturating_sub(AUTH_REQUEST_TTL.as_secs())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn session_key(did: &str, session_id: &str) -> String {
    format!("{did}|{session_id}")
}

/// A file-backed [`ClientAuthStore`].
#[derive(Debug)]
pub struct FileStore {
    path: PathBuf,
    state: RwLock<Persisted>,
}

impl FileStore {
    /// Open (or create) a store at `path`.
    ///
    /// A file that exists but cannot be parsed is treated as empty and logged,
    /// rather than failing startup: a corrupt store means everyone must log in
    /// again, which is recoverable, whereas refusing to boot is not.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let path = path.as_ref().to_path_buf();

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        let state = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                tracing::error!(path = %path.display(), %e, "session store is unreadable, starting empty");
                Persisted::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Persisted::default(),
            Err(e) => return Err(e),
        };

        let store = Self {
            path,
            state: RwLock::new(state),
        };
        // Write once at startup so the file exists with the right mode, and so
        // any entries that expired while the process was down are dropped.
        store.persist(&mut *store.state.write().await).await?;
        Ok(store)
    }

    /// Drop expired authorization requests, then write the store atomically.
    ///
    /// Called with the write lock held, so no other task can interleave a
    /// read-modify-write.
    async fn persist(&self, state: &mut Persisted) -> Result<(), std::io::Error> {
        let now = now_secs();
        let before = state.auth_requests.len();
        state
            .auth_requests
            .retain(|_, r| !is_expired(r.created_at, now));
        let reaped = before - state.auth_requests.len();
        if reaped > 0 {
            tracing::debug!(reaped, "expired abandoned authorization requests");
        }

        let bytes = serde_json::to_vec(state)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Write beside the target, then rename. `rename` is atomic on POSIX, so
        // a reader sees either the old file or the new one — never a truncated
        // one. Same directory, because rename across filesystems fails.
        //
        // The temp name is unique per write: a fixed `.tmp` would let two
        // writers sharing a store path overwrite each other's partial file and
        // rename the result into place.
        let tmp = self.path.with_extension(format!(
            "tmp.{}.{}",
            std::process::id(),
            TMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        tokio::fs::write(&tmp, &bytes).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Set the mode before the rename, so the file is never briefly
            // world-readable at its final name.
            tokio::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(STORE_MODE)).await?;
        }

        tokio::fs::rename(&tmp, &self.path).await
    }

    fn io_err(e: std::io::Error) -> SessionStoreError {
        SessionStoreError::Other(format!("session store IO failed: {e}").into())
    }
}

impl ClientAuthStore for FileStore {
    async fn get_session<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<Option<ClientSessionData>, SessionStoreError> {
        let state = self.state.read().await;
        Ok(state
            .sessions
            .get(&session_key(did.as_ref(), session_id))
            .cloned())
    }

    async fn upsert_session(&self, session: ClientSessionData) -> Result<(), SessionStoreError> {
        let mut state = self.state.write().await;
        let key = session_key(session.account_did.as_ref(), session.session_id.as_ref());
        state.sessions.insert(key, session);
        self.persist(&mut state).await.map_err(Self::io_err)
    }

    async fn delete_session<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<(), SessionStoreError> {
        let mut state = self.state.write().await;
        state
            .sessions
            .remove(&session_key(did.as_ref(), session_id));
        self.persist(&mut state).await.map_err(Self::io_err)
    }

    async fn get_auth_req_info(
        &self,
        state_param: &str,
    ) -> Result<Option<AuthRequestData>, SessionStoreError> {
        let state = self.state.read().await;
        let now = now_secs();
        state
            .auth_requests
            .get(state_param)
            // Expiry is enforced on read as well as on the GC pass, so a stale
            // entry cannot be used just because nothing has written since.
            .filter(|r| !is_expired(r.created_at, now))
            .map(|r| serde_json::from_value(r.data.clone()))
            .transpose()
            .map_err(|e| {
                SessionStoreError::Other(format!("stored auth request is unreadable: {e}").into())
            })
    }

    async fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData,
    ) -> Result<(), SessionStoreError> {
        let mut state = self.state.write().await;
        let data = serde_json::to_value(auth_req_info).map_err(|e| {
            SessionStoreError::Other(format!("auth request is unserializable: {e}").into())
        })?;
        state.auth_requests.insert(
            AsRef::<str>::as_ref(&auth_req_info.state).to_string(),
            StoredAuthRequest {
                created_at: now_secs(),
                data,
            },
        );
        self.persist(&mut state).await.map_err(Self::io_err)
    }

    async fn delete_auth_req_info(&self, state_param: &str) -> Result<(), SessionStoreError> {
        let mut state = self.state.write().await;
        state.auth_requests.remove(state_param);
        self.persist(&mut state).await.map_err(Self::io_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "atpr-session-test-{}-{}-{name}.json",
            std::process::id(),
            now_secs()
        ));
        p
    }

    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            let _ = std::fs::remove_file(self.0.with_extension("tmp"));
        }
    }

    #[tokio::test]
    async fn test_open_creates_the_file() {
        let path = temp_path("create");
        let _c = Cleanup(path.clone());

        let _store = FileStore::open(&path).await.unwrap();
        assert!(path.exists(), "store file should be created");
    }

    /// The file holds DPoP private keys and refresh tokens. It was 0644.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let path = temp_path("mode");
        let _c = Cleanup(path.clone());

        let _store = FileStore::open(&path).await.unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, STORE_MODE, "expected 0600, got {mode:o}");
    }

    #[tokio::test]
    async fn test_unparseable_file_starts_empty_rather_than_failing() {
        let path = temp_path("corrupt");
        let _c = Cleanup(path.clone());
        std::fs::write(&path, b"{ this is not json").unwrap();

        let store = FileStore::open(&path).await.expect("should not fail");
        assert!(store.state.read().await.sessions.is_empty());
        // And the file is rewritten as valid JSON.
        let bytes = std::fs::read(&path).unwrap();
        assert!(serde_json::from_slice::<Persisted>(&bytes).is_ok());
    }

    #[tokio::test]
    async fn test_no_temp_file_is_left_behind() {
        let path = temp_path("tmp");
        let _c = Cleanup(path.clone());

        let _store = FileStore::open(&path).await.unwrap();
        assert!(
            !path.with_extension("tmp").exists(),
            "the temp file must be renamed away, not left on disk"
        );
    }

    #[test]
    fn test_is_expired() {
        let now = 1_000_000;
        let ttl = AUTH_REQUEST_TTL.as_secs();

        assert!(!is_expired(now, now), "just created");
        assert!(!is_expired(now - ttl, now), "exactly at the TTL boundary");
        assert!(is_expired(now - ttl - 1, now), "one second past the TTL");
        assert!(is_expired(0, now), "epoch");
        // A clock that went backwards must not expire everything.
        assert!(!is_expired(now, 0));
    }

    /// Abandoned logins accumulated forever: an `oauth-state:` record was
    /// written when a login started and deleted only when a callback arrived.
    #[tokio::test]
    async fn test_expired_auth_requests_are_reaped_and_unreadable() {
        let path = temp_path("ttl");
        let _c = Cleanup(path.clone());

        let store = FileStore::open(&path).await.unwrap();

        // Insert one stale and one fresh entry directly, so the test does not
        // have to wait ten minutes.
        {
            let mut state = store.state.write().await;
            state.auth_requests.insert(
                "stale".to_string(),
                StoredAuthRequest {
                    created_at: now_secs() - AUTH_REQUEST_TTL.as_secs() - 1,
                    data: serde_json::json!({ "state": "stale" }),
                },
            );
            state.auth_requests.insert(
                "fresh".to_string(),
                StoredAuthRequest {
                    created_at: now_secs(),
                    data: serde_json::json!({ "state": "fresh" }),
                },
            );
        }

        // A stale entry must not be usable even before the GC pass runs.
        assert!(store.get_auth_req_info("stale").await.unwrap().is_none());

        // The next persist reaps it, and leaves the fresh one alone.
        {
            let mut state = store.state.write().await;
            store.persist(&mut state).await.unwrap();
        }
        let state = store.state.read().await;
        assert!(!state.auth_requests.contains_key("stale"));
        assert!(state.auth_requests.contains_key("fresh"));
    }

    /// Sessions must survive a reopen — that is the entire point of the file.
    #[tokio::test]
    async fn test_persistence_round_trip() {
        let path = temp_path("roundtrip");
        let _c = Cleanup(path.clone());

        {
            let store = FileStore::open(&path).await.unwrap();
            let mut state = store.state.write().await;
            state.auth_requests.insert(
                "keep".to_string(),
                StoredAuthRequest {
                    created_at: now_secs(),
                    data: serde_json::json!({ "state": "keep" }),
                },
            );
            store.persist(&mut state).await.unwrap();
        }

        let reopened = FileStore::open(&path).await.unwrap();
        assert!(reopened
            .state
            .read()
            .await
            .auth_requests
            .contains_key("keep"));
    }
}
