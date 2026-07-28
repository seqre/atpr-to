//! OAuth session persistence.
//!
//! This is the one place the design keeps enum dispatch rather than a trait
//! parameter: jacquard's `ClientAuthStore` has generic methods, so it is not
//! object-safe, and threading another type parameter through `AppState` for a
//! choice made once at startup is not worth it.
//!
//! The seam is deliberate. A shared backend (DynamoDB) is the only real fix for
//! sessions being per-execution-environment on Lambda — PAR state written by
//! one instance is missing when the callback lands on another, so logins fail
//! nondeterministically. That fix is a third variant here plus its
//! `ClientAuthStore` impl, and nothing else changes.

pub mod file;

use jacquard::oauth::authstore::{ClientAuthStore, MemoryAuthStore};
use jacquard::oauth::session::{AuthRequestData, ClientSessionData};
use jacquard_common::bos::BosStr;
use jacquard_common::session::SessionStoreError;
use jacquard_common::types::did::Did;

pub use file::FileStore;

/// Where OAuth sessions live.
pub enum AuthStore {
    /// In-memory; sessions are lost on restart.
    Memory(MemoryAuthStore),
    /// File-backed; see [`FileStore`].
    File(FileStore),
}

/// Forward a `ClientAuthStore` call to whichever variant is active.
///
/// Replaces sixty lines of hand-written match arms that differed only in the
/// method name — six near-identical blocks, each a place to make a typo.
macro_rules! delegate {
    ($self:expr, $method:ident ( $($arg:expr),* $(,)? )) => {
        match $self {
            AuthStore::Memory(s) => s.$method($($arg),*).await,
            AuthStore::File(s) => s.$method($($arg),*).await,
        }
    };
}

impl ClientAuthStore for AuthStore {
    async fn get_session<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<Option<ClientSessionData>, SessionStoreError> {
        delegate!(self, get_session(did, session_id))
    }

    async fn upsert_session(&self, session: ClientSessionData) -> Result<(), SessionStoreError> {
        delegate!(self, upsert_session(session))
    }

    async fn delete_session<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<(), SessionStoreError> {
        delegate!(self, delete_session(did, session_id))
    }

    async fn get_auth_req_info(
        &self,
        state: &str,
    ) -> Result<Option<AuthRequestData>, SessionStoreError> {
        delegate!(self, get_auth_req_info(state))
    }

    async fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData,
    ) -> Result<(), SessionStoreError> {
        delegate!(self, save_auth_req_info(auth_req_info))
    }

    async fn delete_auth_req_info(&self, state: &str) -> Result<(), SessionStoreError> {
        delegate!(self, delete_auth_req_info(state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "atpr-authstore-test-{}-{name}.json",
            std::process::id()
        ));
        p
    }

    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// Every arm of the delegation macro, against both variants.
    ///
    /// The macro replaced sixty lines of hand-written match arms; a typo in one
    /// of them — forwarding `delete_session` to `get_session`, say — would
    /// compile and silently do the wrong thing, so exercise each method through
    /// the enum rather than trusting it by inspection.
    async fn exercise(store: &AuthStore) {
        let did: Did = Did::new_owned("did:plc:testdelegate").unwrap();

        // Reads on an empty store answer None rather than erroring.
        assert!(store.get_session(&did, "no-such").await.unwrap().is_none());
        assert!(store.get_auth_req_info("no-such").await.unwrap().is_none());

        // Deletes of absent keys are not errors.
        store.delete_session(&did, "no-such").await.unwrap();
        store.delete_auth_req_info("no-such").await.unwrap();
    }

    #[tokio::test]
    async fn test_memory_variant_delegates() {
        exercise(&AuthStore::Memory(MemoryAuthStore::new())).await;
    }

    #[tokio::test]
    async fn test_file_variant_delegates() {
        let path = temp_path("delegate");
        let _c = Cleanup(path.clone());
        exercise(&AuthStore::File(FileStore::open(&path).await.unwrap())).await;
    }
}
