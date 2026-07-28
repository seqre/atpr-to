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
