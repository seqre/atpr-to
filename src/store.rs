//! The write side: where an authenticated user's links live.
//!
//! `LinkStore` is the seam that makes the write path testable. Handlers used to
//! call jacquard's `OAuthSession` inline, so exercising `shorten` or
//! `delete_link` required a live PDS — which is why both were untested and
//! annotated `coverage:excl` rather than covered.
//!
//! The trait uses `-> impl Future<Output = ...> + Send` rather than `async fn`
//! because a bare `async fn` in a trait cannot require `Send` on the returned
//! future, and axum needs it. That is the real reason people reach for
//! `#[async_trait]`; the explicit form solves it with no macro and no boxing.
//! The cost is that the trait is not `dyn`-compatible, so `AppState` is generic
//! over the authenticator instead of holding a `Box<dyn LinkStore>`.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Mutex;

use jacquard::api::com_atproto::repo::delete_record::DeleteRecord;
use jacquard::api::com_atproto::repo::list_records::ListRecords;
use jacquard::api::com_atproto::repo::put_record::{PutRecord, PutRecordError};
use jacquard_common::deps::smol_str::SmolStr;
use jacquard_common::types::collection::Collection;
use jacquard_common::types::did::Did;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::types::recordkey::{RecordKey, Rkey};
use jacquard_common::types::string::Datetime;
use jacquard_common::types::uri::UriValue;
use jacquard_common::types::value::{to_data, Data};
use jacquard_common::xrpc::{XrpcClient, XrpcError};

use crate::auth::OAuthSessionType;
use crate::domain::{ShortCode, ShortLink, TargetUrl};
use crate::error::AppError;
use crate::generated::to_atpr::link::Link;

/// Why a store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The short code is already taken.
    #[error("short code already in use")]
    Conflict,
    /// No record exists under that short code.
    #[error("no such link")]
    NotFound,
    /// The PDS could not be reached, or answered with something unexpected.
    #[error(transparent)]
    Upstream(#[from] anyhow::Error),
}

impl From<StoreError> for AppError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Conflict => AppError::Conflict,
            StoreError::NotFound => AppError::NotFound,
            StoreError::Upstream(e) => AppError::Upstream(e),
        }
    }
}

/// Whether a write may replace an existing record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PutMode {
    /// Fail with [`StoreError::Conflict`] if the code is already in use.
    CreateOnly,
    /// Replace whatever is there.
    Overwrite,
}

/// Maximum page size the AT Protocol `listRecords` endpoint accepts.
pub const MAX_PAGE_SIZE: u8 = 100;

/// A request for one page of links.
#[derive(Debug, Clone)]
pub struct PageRequest {
    /// How many records to fetch. Clamped to [`MAX_PAGE_SIZE`].
    pub limit: u8,
    /// Opaque cursor from a previous [`LinkPage`].
    pub cursor: Option<String>,
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            limit: MAX_PAGE_SIZE,
            cursor: None,
        }
    }
}

/// One page of a user's links.
#[derive(Debug, Clone, Default)]
pub struct LinkPage {
    /// The links in this page.
    pub links: Vec<LinkEntry>,
    /// Cursor for the next page, or `None` at the end.
    ///
    /// This used to be discarded, and `listRecords` was called with no `limit`
    /// either — so a user with more than the server default of 50 links
    /// silently saw only the first 50.
    pub cursor: Option<String>,
}

/// A single stored link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkEntry {
    /// The short code.
    pub code: ShortCode,
    /// Where it points, and when it last changed.
    pub link: ShortLink,
}

/// Read and write a user's short links.
pub trait LinkStore: Send + Sync {
    /// Create or replace a link.
    fn put(
        &self,
        code: &ShortCode,
        target: &TargetUrl,
        mode: PutMode,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// Fetch one page of links.
    fn list(&self, page: PageRequest) -> impl Future<Output = Result<LinkPage, StoreError>> + Send;

    /// Delete a link.
    fn delete(&self, code: &ShortCode) -> impl Future<Output = Result<(), StoreError>> + Send;
}

/// So a caller can hand out shared handles to one store — which is how tests
/// keep a reference to inspect after the request has consumed its copy.
impl<T: LinkStore> LinkStore for std::sync::Arc<T> {
    fn put(
        &self,
        code: &ShortCode,
        target: &TargetUrl,
        mode: PutMode,
    ) -> impl Future<Output = Result<(), StoreError>> + Send {
        (**self).put(code, target, mode)
    }

    fn list(&self, page: PageRequest) -> impl Future<Output = Result<LinkPage, StoreError>> + Send {
        (**self).list(page)
    }

    fn delete(&self, code: &ShortCode) -> impl Future<Output = Result<(), StoreError>> + Send {
        (**self).delete(code)
    }
}

/// Extract the record key from an AT-URI (`at://{did}/{collection}/{rkey}`).
pub fn rkey_from_at_uri(at_uri: &str) -> &str {
    at_uri.split('/').next_back().unwrap_or("")
}

/// A [`LinkStore`] backed by the user's own PDS repo.
pub struct PdsLinkStore {
    session: OAuthSessionType,
    did: Did,
}

impl PdsLinkStore {
    /// Wrap an authenticated session as a link store.
    pub fn new(session: OAuthSessionType, did: Did) -> Self {
        Self { session, did }
    }

    /// The `to.atpr.link` collection NSID.
    fn collection() -> Nsid {
        Nsid::new_static(<Link as Collection>::NSID).expect("generated NSID is valid")
    }

    /// The repo identifier for this user.
    ///
    /// Replaces the `Did::new_owned` + `Nsid::new_static` + `AtIdentifier::Did`
    /// preamble that was triplicated across shorten, delete and links.
    fn repo(&self) -> AtIdentifier {
        AtIdentifier::Did(self.did.clone())
    }

    /// Build a record key from a validated short code.
    fn rkey(code: &ShortCode) -> Result<RecordKey<Rkey>, StoreError> {
        // Unreachable: `ShortCode`'s charset is a subset of the rkey charset.
        RecordKey::any_owned(code.as_str())
            .map_err(|e| StoreError::Upstream(anyhow::anyhow!("invalid record key: {e}")))
    }
}

impl LinkStore for PdsLinkStore {
    async fn put(
        &self,
        code: &ShortCode,
        target: &TargetUrl,
        mode: PutMode,
    ) -> Result<(), StoreError> {
        let link_url = UriValue::new_owned(target.as_str())
            .map_err(|e| StoreError::Upstream(anyhow::anyhow!("invalid target URI: {e}")))?;

        let record: Link = Link::new()
            .url(link_url)
            .updated_at(Datetime::now())
            .build();
        let data = to_data(&record)
            .map_err(|e| StoreError::Upstream(anyhow::anyhow!("failed to encode record: {e}")))?;

        let mut request = PutRecord::new()
            .repo(self.repo())
            .collection(Self::collection())
            .rkey(Self::rkey(code)?)
            .record(data)
            .build();

        if mode == PutMode::CreateOnly {
            // `putRecord` is an upsert by default, so re-submitting a custom
            // code silently destroyed the previous link — and generated codes
            // collided the same way, undetected.
            //
            // `swapRecord: null` means "must not already exist", which makes
            // this a compare-and-swap rather than a read-then-write race. The
            // typed `swap_record: Option<Cid>` field cannot express it: it is
            // `skip_serializing_if = "Option::is_none"`, so `None` omits the
            // key rather than emitting a null. Going through the flattened
            // `extra_data` map produces the explicit null. There is a test
            // asserting the serialized request still contains it.
            let mut extra: BTreeMap<SmolStr, Data> = BTreeMap::new();
            extra.insert(SmolStr::new("swapRecord"), Data::Null);
            request.extra_data = Some(extra);
        }

        let response = self
            .session
            .send(request)
            .await
            .map_err(|e| StoreError::Upstream(anyhow::anyhow!("putRecord failed: {e}")))?;

        match response.into_output() {
            Ok(_) => Ok(()),
            // Typed, not a string match: the PDS reports a failed compare-and-swap
            // as `InvalidSwap`, which here can only mean the code was taken.
            Err(XrpcError::Xrpc(PutRecordError::InvalidSwap(_))) => Err(StoreError::Conflict),
            Err(e) => Err(StoreError::Upstream(anyhow::anyhow!(
                "putRecord rejected: {e}"
            ))),
        }
    }

    async fn list(&self, page: PageRequest) -> Result<LinkPage, StoreError> {
        let limit = page.limit.clamp(1, MAX_PAGE_SIZE);
        let mut request = ListRecords::new()
            .repo(self.repo())
            .collection(Self::collection())
            .build();
        request.limit = Some(i64::from(limit));
        request.cursor = page.cursor.map(SmolStr::new);

        let response = self
            .session
            .send(request)
            .await
            .map_err(|e| StoreError::Upstream(anyhow::anyhow!("listRecords failed: {e}")))?;

        let output = response
            .into_output()
            .map_err(|e| StoreError::Upstream(anyhow::anyhow!("listRecords rejected: {e}")))?;

        let links = output
            .records
            .iter()
            .filter_map(|record| {
                let code = ShortCode::parse(rkey_from_at_uri(record.uri.as_ref()))
                    .inspect_err(|e| tracing::warn!(uri = %record.uri.as_ref(), %e, "skipping record with an unusable rkey"))
                    .ok()?;

                let value = serde_json::to_value(&record.value)
                    .inspect_err(|e| tracing::warn!(%e, "skipping unserializable record"))
                    .ok()?;

                let target = TargetUrl::parse(value.get("url").and_then(|u| u.as_str())?)
                    .inspect_err(|e| tracing::warn!(%code, %e, "skipping record with an unusable destination"))
                    .ok()?;

                let updated_at = value
                    .get("updatedAt")
                    .and_then(|c| c.as_str())
                    .map(str::to_owned);

                Some(LinkEntry {
                    code,
                    link: ShortLink { target, updated_at },
                })
            })
            .collect();

        // A short page is the last page. The PDS returns a cursor either way --
        // `listRecords` hands one back whenever it returned any records at all,
        // including on the final page -- and a caller cannot tell the difference,
        // so the dashboard offered "Load more" on every list it ever drew.
        // `InMemoryLinkStore::list` already suppresses it; this is the PDS side
        // of the same contract.
        //
        // Counted on `output.records`, not on `links`: the `filter_map` above
        // drops records with an unusable rkey or destination, and a page that
        // arrived full is still a full page even if we could not use all of it.
        let cursor = if output.records.len() < usize::from(limit) {
            None
        } else {
            output.cursor.map(|c| c.to_string())
        };

        Ok(LinkPage { links, cursor })
    }

    async fn delete(&self, code: &ShortCode) -> Result<(), StoreError> {
        let request = DeleteRecord::new()
            .repo(self.repo())
            .collection(Self::collection())
            .rkey(Self::rkey(code)?)
            .build();

        self.session
            .send(request)
            .await
            .map_err(|e| StoreError::Upstream(anyhow::anyhow!("deleteRecord failed: {e}")))?
            .into_output()
            .map_err(|e| StoreError::Upstream(anyhow::anyhow!("deleteRecord rejected: {e}")))?;

        Ok(())
    }
}

/// An in-memory [`LinkStore`] for tests.
///
/// Lives here rather than behind `#[cfg(test)]` because the integration tests in
/// `tests/` are a separate crate and need it too.
#[derive(Debug, Default)]
pub struct InMemoryLinkStore {
    links: Mutex<BTreeMap<String, ShortLink>>,
    /// When set, every operation fails with this message instead.
    fail_with: Option<String>,
}

impl InMemoryLinkStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// A store whose every operation fails, for exercising the upstream path.
    pub fn failing(message: impl Into<String>) -> Self {
        Self {
            links: Mutex::new(BTreeMap::new()),
            fail_with: Some(message.into()),
        }
    }

    /// Insert a link directly, bypassing the trait.
    pub fn insert(&self, code: &str, target: &str) {
        self.links.lock().expect("not poisoned").insert(
            code.to_string(),
            ShortLink {
                target: TargetUrl::parse(target).expect("test fixture is a valid target"),
                updated_at: Some("2024-01-01T00:00:00Z".to_string()),
            },
        );
    }

    /// How many links are stored.
    pub fn len(&self) -> usize {
        self.links.lock().expect("not poisoned").len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The destination stored under `code`, if any.
    pub fn get(&self, code: &str) -> Option<String> {
        self.links
            .lock()
            .expect("not poisoned")
            .get(code)
            .map(|l| l.target.to_string())
    }

    fn check_failure(&self) -> Result<(), StoreError> {
        match &self.fail_with {
            Some(m) => Err(StoreError::Upstream(anyhow::anyhow!("{m}"))),
            None => Ok(()),
        }
    }
}

impl LinkStore for InMemoryLinkStore {
    async fn put(
        &self,
        code: &ShortCode,
        target: &TargetUrl,
        mode: PutMode,
    ) -> Result<(), StoreError> {
        self.check_failure()?;
        let mut links = self.links.lock().expect("not poisoned");
        if mode == PutMode::CreateOnly && links.contains_key(code.as_str()) {
            return Err(StoreError::Conflict);
        }
        links.insert(
            code.as_str().to_string(),
            ShortLink {
                target: target.clone(),
                updated_at: Some("2024-01-01T00:00:00Z".to_string()),
            },
        );
        Ok(())
    }

    async fn list(&self, page: PageRequest) -> Result<LinkPage, StoreError> {
        self.check_failure()?;
        let links = self.links.lock().expect("not poisoned");

        let limit = usize::from(page.limit.clamp(1, MAX_PAGE_SIZE));
        let start = page.cursor.as_deref().unwrap_or("");

        let mut page_links: Vec<LinkEntry> = links
            .iter()
            .filter(|(code, _)| code.as_str() > start || start.is_empty())
            .take(limit + 1)
            .map(|(code, link)| LinkEntry {
                code: ShortCode::parse(code).expect("stored codes are valid"),
                link: link.clone(),
            })
            .collect();

        let cursor = if page_links.len() > limit {
            page_links.truncate(limit);
            page_links.last().map(|e| e.code.as_str().to_string())
        } else {
            None
        };

        Ok(LinkPage {
            links: page_links,
            cursor,
        })
    }

    async fn delete(&self, code: &ShortCode) -> Result<(), StoreError> {
        self.check_failure()?;
        let mut links = self.links.lock().expect("not poisoned");
        match links.remove(code.as_str()) {
            Some(_) => Ok(()),
            None => Err(StoreError::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(s: &str) -> ShortCode {
        ShortCode::parse(s).unwrap()
    }

    fn target(s: &str) -> TargetUrl {
        TargetUrl::parse(s).unwrap()
    }

    #[test]
    fn test_rkey_from_at_uri() {
        assert_eq!(
            rkey_from_at_uri("at://did:plc:abc123/to.atpr.link/mycode"),
            "mycode"
        );
        assert_eq!(
            rkey_from_at_uri("at://did:plc:abc123/to.atpr.link/abc-123_XY"),
            "abc-123_XY"
        );
        assert_eq!(rkey_from_at_uri(""), "");
    }

    /// Bug #4: `putRecord` is an upsert, so without a compare-and-swap a
    /// re-used code silently destroys the previous link.
    #[tokio::test]
    async fn test_create_only_rejects_an_existing_code() {
        let store = InMemoryLinkStore::new();
        store
            .put(
                &code("dup"),
                &target("https://first.example"),
                PutMode::CreateOnly,
            )
            .await
            .unwrap();

        let err = store
            .put(
                &code("dup"),
                &target("https://second.example"),
                PutMode::CreateOnly,
            )
            .await
            .expect_err("second create must conflict");
        assert!(matches!(err, StoreError::Conflict));

        assert_eq!(
            store.get("dup").as_deref(),
            Some("https://first.example/"),
            "the original link must survive"
        );
    }

    #[tokio::test]
    async fn test_overwrite_replaces() {
        let store = InMemoryLinkStore::new();
        store
            .put(
                &code("x"),
                &target("https://first.example"),
                PutMode::CreateOnly,
            )
            .await
            .unwrap();
        store
            .put(
                &code("x"),
                &target("https://second.example"),
                PutMode::Overwrite,
            )
            .await
            .unwrap();
        assert_eq!(store.get("x").as_deref(), Some("https://second.example/"));
    }

    #[tokio::test]
    async fn test_delete_missing_is_not_found() {
        let store = InMemoryLinkStore::new();
        let err = store.delete(&code("nope")).await.expect_err("should fail");
        assert!(matches!(err, StoreError::NotFound));
    }

    #[tokio::test]
    async fn test_delete_removes() {
        let store = InMemoryLinkStore::new();
        store
            .put(
                &code("gone"),
                &target("https://example.com"),
                PutMode::CreateOnly,
            )
            .await
            .unwrap();
        store.delete(&code("gone")).await.unwrap();
        assert!(store.is_empty());
    }

    /// Bug #5: more links than one page must remain reachable.
    #[tokio::test]
    async fn test_list_paginates_past_the_page_size() {
        let store = InMemoryLinkStore::new();
        for i in 0..120 {
            store.insert(&format!("code{i:03}"), "https://example.com");
        }

        let first = store
            .list(PageRequest {
                limit: 50,
                cursor: None,
            })
            .await
            .unwrap();
        assert_eq!(first.links.len(), 50);
        let cursor = first.cursor.expect("more pages remain");

        let second = store
            .list(PageRequest {
                limit: 50,
                cursor: Some(cursor),
            })
            .await
            .unwrap();
        assert_eq!(second.links.len(), 50);
        assert!(second.cursor.is_some());

        let third = store
            .list(PageRequest {
                limit: 50,
                cursor: second.cursor,
            })
            .await
            .unwrap();
        assert_eq!(third.links.len(), 20);
        assert!(third.cursor.is_none(), "last page has no cursor");

        let total = first.links.len() + second.links.len() + third.links.len();
        assert_eq!(total, 120, "every link must be reachable");
    }

    #[tokio::test]
    async fn test_list_clamps_limit() {
        let store = InMemoryLinkStore::new();
        for i in 0..150 {
            store.insert(&format!("code{i:03}"), "https://example.com");
        }
        let page = store
            .list(PageRequest {
                limit: 255,
                cursor: None,
            })
            .await
            .unwrap();
        assert_eq!(page.links.len(), usize::from(MAX_PAGE_SIZE));
    }

    #[tokio::test]
    async fn test_failing_store_reports_upstream() {
        let store = InMemoryLinkStore::failing("PDS is down");
        let err = store
            .put(
                &code("x"),
                &target("https://example.com"),
                PutMode::CreateOnly,
            )
            .await
            .expect_err("should fail");
        assert!(matches!(err, StoreError::Upstream(_)));
        assert_eq!(
            AppError::from(err).status(),
            axum::http::StatusCode::BAD_GATEWAY
        );
    }

    #[test]
    fn test_store_error_status_mapping() {
        use axum::http::StatusCode;
        assert_eq!(
            AppError::from(StoreError::Conflict).status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            AppError::from(StoreError::NotFound).status(),
            StatusCode::NOT_FOUND
        );
    }

    /// The `swapRecord: null` trick relies on flattened `extra_data`, because
    /// the typed field is `skip_serializing_if = "Option::is_none"`. If a
    /// jacquard upgrade ever makes the typed field able to express it, this
    /// test will still pass — but if flattening stops emitting the key, the
    /// compare-and-swap silently degrades to an upsert, which is exactly the
    /// bug. So assert on the wire format.
    #[test]
    fn test_create_only_serializes_an_explicit_swap_record_null() {
        let did: Did = Did::new_owned("did:plc:test123").unwrap();
        let record: Link = Link::new()
            .url(UriValue::new_owned("https://example.com").unwrap())
            .updated_at(Datetime::now())
            .build();

        let mut request = PutRecord::new()
            .repo(AtIdentifier::Did(did))
            .collection(PdsLinkStore::collection())
            .rkey(PdsLinkStore::rkey(&code("abc123")).unwrap())
            .record(to_data(&record).unwrap())
            .build();

        let without = serde_json::to_value(&request).unwrap();
        assert!(
            !without.as_object().unwrap().contains_key("swapRecord"),
            "Overwrite mode must not send swapRecord"
        );

        let mut extra: BTreeMap<SmolStr, Data> = BTreeMap::new();
        extra.insert(SmolStr::new("swapRecord"), Data::Null);
        request.extra_data = Some(extra);

        let with = serde_json::to_value(&request).unwrap();
        assert_eq!(
            with.get("swapRecord"),
            Some(&serde_json::Value::Null),
            "CreateOnly must send an explicit JSON null, not an absent key"
        );
    }
}
