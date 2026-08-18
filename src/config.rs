//! Application configuration: compiled defaults → `Config.toml` → `ATPR__` env vars.
//!
//! Loading is fail-fast. It used to fall back to compiled defaults on *any*
//! error, which meant a malformed `Config.toml` booted the app with
//! `base_url = https://atpr.to` — for a self-hoster, silently minting short
//! URLs pointing at someone else's domain. Refusing to start is the only safe
//! response to configuration you cannot parse.
//!
//! Types carry their own invariants, so validation is deserialization:
//! `BaseUrl` cannot hold an unparseable URL, `NonZeroU64` cannot hold the zero
//! that used to panic the rate limiter on cold start, and `SessionStore` makes
//! "empty string means in-memory" an explicit variant instead of a convention.

use std::num::{NonZeroU32, NonZeroU64};
use std::path::PathBuf;

use serde::{Deserialize, Deserializer};
use url::Url;

/// The public origin this instance serves from.
///
/// Parsed once at startup, so the three `Uri::parse(...).unwrap()` calls that
/// used to re-parse it are unconstructible.
#[derive(Debug, Clone)]
pub struct BaseUrl {
    url: Url,
    /// `url` rendered without a trailing slash, which is what every call site
    /// wants — `Url` normalises `https://atpr.to` to `https://atpr.to/`, and
    /// concatenating that yields a double slash.
    trimmed: String,
}

impl BaseUrl {
    /// Parse and validate a base URL.
    pub fn parse(s: &str) -> Result<Self, url::ParseError> {
        Ok(Self::from_url(Url::parse(s)?))
    }

    fn from_url(url: Url) -> Self {
        let trimmed = url.as_str().trim_end_matches('/').to_string();
        Self { url, trimmed }
    }

    /// The base URL as a string, without a trailing slash.
    pub fn as_str(&self) -> &str {
        &self.trimmed
    }

    /// The underlying parsed URL.
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Build the canonical short URL for a handle and code.
    ///
    /// Single definition of the `{base}/@{handle}/{code}` shape, which was
    /// spelled out as an inline `format!` in three separate modules.
    pub fn short_url(&self, handle: &str, code: &str) -> String {
        format!("{}/@{}/{}", self.trimmed, handle, code)
    }

    /// True when this points at a loopback address.
    ///
    /// Checked against the parsed host rather than by string prefix, so
    /// `http://localhost.evil.com` is not mistaken for local development.
    pub fn is_loopback(&self) -> bool {
        self.url.scheme() == "http"
            && matches!(self.url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
    }
}

impl std::fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.trimmed)
    }
}

impl<'de> Deserialize<'de> for BaseUrl {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Url::deserialize(d).map(Self::from_url)
    }
}

/// Where OAuth sessions are persisted.
///
/// Replaces a `session_file: String` in which `""` silently meant "in memory".
/// The wire format keeps the same key so existing `Config.toml` files and the
/// `ATPR__SESSION_FILE` env var in `template.yaml` continue to work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStore {
    /// Sessions live in process memory and are lost on restart.
    Memory,
    /// Sessions are persisted to a file.
    File(PathBuf),
    /// Sessions live in a DynamoDB table, shared by every instance.
    ///
    /// The only variant that works on Lambda: the other two are per execution
    /// environment, so an authorization request written while starting a login
    /// is missing when the callback lands on a different instance.
    Dynamo(String),
}

/// Prefix that selects the DynamoDB store, followed by the table name.
const DYNAMO_SCHEME: &str = "dynamodb://";

impl SessionStore {
    /// The backing file path, if this is a file-backed store.
    pub fn path(&self) -> Option<&std::path::Path> {
        match self {
            Self::File(p) => Some(p),
            _ => None,
        }
    }

    /// The table name, if this is a DynamoDB-backed store.
    pub fn table(&self) -> Option<&str> {
        match self {
            Self::Dynamo(t) => Some(t),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for SessionStore {
    /// One key, three shapes.
    ///
    /// The wire format stays a single `session_file` string because
    /// `template.yaml` and every existing `Config.toml` and `ATPR__SESSION_FILE`
    /// already set it: `""` is memory, `dynamodb://name` is the table `name`,
    /// and anything else is a path. A second key would have made two ways to
    /// say where sessions live, and a rule about which one wins.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Ok(match raw.strip_prefix(DYNAMO_SCHEME) {
            Some(table) if !table.is_empty() => Self::Dynamo(table.to_string()),
            // `dynamodb://` with nothing after it names no table. Falling
            // through to `File("dynamodb://")` would create that file and look
            // like it had worked.
            Some(_) => {
                return Err(serde::de::Error::custom(
                    "session_file: `dynamodb://` needs a table name after it",
                ))
            }
            None if raw.is_empty() => Self::Memory,
            None => Self::File(PathBuf::from(raw)),
        })
    }
}

/// Rate limiting configuration for mutation routes.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct RateLimitConfig {
    /// Sustained request rate in requests per second.
    ///
    /// `NonZeroU64` because `GovernorConfigBuilder::finish()` returns `None` for
    /// a zero rate, and the resulting `.unwrap()` panicked on every cold start.
    /// The bug is now unrepresentable rather than merely caught.
    pub per_second: NonZeroU64,
    /// Maximum burst above the sustained rate.
    pub burst_size: NonZeroU32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            per_second: NonZeroU64::new(2).expect("2 is nonzero"),
            burst_size: NonZeroU32::new(10).expect("10 is nonzero"),
        }
    }
}

/// Application configuration.
///
/// `#[serde(default)]` means missing fields come from `Default`, so the compiled
/// defaults have exactly one definition. Previously `impl Default` and a chain
/// of `set_default` calls in `load()` hardcoded the same six values
/// independently, free to drift apart.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Base URL for short links and OAuth metadata (e.g. `https://atpr.to`).
    pub base_url: BaseUrl,
    /// Slingshot relay URL used for fast AT Protocol resolution.
    pub slingshot_url: String,
    /// Bluesky AppView base URL, used only to look up a dashboard avatar.
    ///
    /// Configurable for the same reason `slingshot_url` is: hardcoded into the
    /// handler, it made the dashboard's authenticated path impossible to test
    /// without reaching the public internet.
    pub appview_url: String,
    /// Rate limiting parameters for mutation routes.
    pub rate_limit: RateLimitConfig,
    /// `Cache-Control: max-age` value (seconds) for static files.
    pub static_cache_max_age: u32,
    /// `Cache-Control: max-age` value (seconds) for successful redirects.
    ///
    /// The redirect *is* the product, and it costs two upstream hops with no
    /// caching at all today. It is a knob rather than a constant because the
    /// cost is real: an edit or a delete takes up to this long to propagate to
    /// anyone who already followed the link. `0` disables caching entirely.
    pub redirect_cache_max_age: u32,
    /// Server-side budget for one inbound request, in milliseconds.
    ///
    /// Sits below the Lambda function timeout deliberately, so a request that
    /// overruns returns a 504 we can see in the logs rather than being killed
    /// by the runtime with no response at all.
    pub request_timeout_ms: NonZeroU64,
    /// Total budget for one outbound request, in milliseconds.
    ///
    /// Worth tuning against the Lambda function timeout: the direct resolution
    /// path makes three sequential hops, so the worst case is roughly three
    /// times this.
    pub http_timeout_ms: NonZeroU64,
    /// Budget for establishing an outbound connection, in milliseconds.
    pub http_connect_timeout_ms: NonZeroU64,
    /// Where OAuth sessions are persisted.
    #[serde(rename = "session_file")]
    pub session_store: SessionStore,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: BaseUrl::parse("https://atpr.to").expect("compiled default is a valid URL"),
            slingshot_url: "https://slingshot.microcosm.blue/".to_string(),
            appview_url: "https://public.api.bsky.app".to_string(),
            rate_limit: RateLimitConfig::default(),
            static_cache_max_age: 15,
            redirect_cache_max_age: 60,
            request_timeout_ms: NonZeroU64::new(25_000).expect("25000 is nonzero"),
            http_timeout_ms: NonZeroU64::new(5_000).expect("5000 is nonzero"),
            http_connect_timeout_ms: NonZeroU64::new(2_000).expect("2000 is nonzero"),
            session_store: SessionStore::Memory,
        }
    }
}

/// Load configuration, or explain why it could not be loaded.
///
/// Priority, last wins: compiled defaults → `Config.toml` → `ATPR__` env vars.
pub fn try_load() -> Result<Config, config::ConfigError> {
    config::Config::builder()
        .add_source(config::File::with_name("Config").required(false))
        .add_source(
            config::Environment::with_prefix("ATPR")
                .prefix_separator("__")
                .separator("__")
                .try_parsing(true),
        )
        .build()?
        .try_deserialize::<Config>()
}

/// Load configuration, aborting the process if it is invalid.
///
/// Called during startup only. Refusing to boot beats booting with defaults
/// that point at the wrong domain.
pub fn load() -> Config {
    match try_load() {
        Ok(cfg) => cfg,
        Err(e) => panic!("invalid configuration: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let c = Config::default();
        assert_eq!(c.base_url.as_str(), "https://atpr.to");
        assert_eq!(c.slingshot_url, "https://slingshot.microcosm.blue/");
        assert_eq!(c.session_store, SessionStore::Memory);
    }

    #[test]
    fn test_rate_limit_defaults() {
        let r = RateLimitConfig::default();
        assert_eq!(r.per_second.get(), 2);
        assert_eq!(r.burst_size.get(), 10);
    }

    #[test]
    fn test_static_cache_max_age_default() {
        assert_eq!(Config::default().static_cache_max_age, 15);
    }

    #[test]
    fn test_cache_and_timeout_defaults() {
        let c = Config::default();
        assert_eq!(c.redirect_cache_max_age, 60);
        // Under the 30s Lambda function timeout, or the layer never fires.
        assert!(c.request_timeout_ms.get() < 30_000);
        // And above the outbound budget, so a slow upstream is attributed to
        // the upstream rather than reported as a server-side overrun.
        assert!(c.request_timeout_ms.get() > c.http_timeout_ms.get());
    }

    /// `0` is a meaningful value here — "do not cache redirects" — so unlike
    /// the rate limits it must stay representable.
    #[test]
    fn test_redirect_cache_may_be_disabled() {
        let cfg: Config = toml::from_str("redirect_cache_max_age = 0").unwrap();
        assert_eq!(cfg.redirect_cache_max_age, 0);
    }

    /// `Url` normalises a bare origin to a trailing slash; concatenating that
    /// produced `https://atpr.to//@alice/code`.
    #[test]
    fn test_base_url_trims_trailing_slash() {
        assert_eq!(
            BaseUrl::parse("https://atpr.to/").unwrap().as_str(),
            "https://atpr.to"
        );
        assert_eq!(
            BaseUrl::parse("https://atpr.to").unwrap().as_str(),
            "https://atpr.to"
        );
    }

    #[test]
    fn test_short_url() {
        let base = BaseUrl::parse("https://atpr.to").unwrap();
        assert_eq!(
            base.short_url("alice.bsky.social", "abc123"),
            "https://atpr.to/@alice.bsky.social/abc123"
        );
    }

    #[test]
    fn test_short_url_from_slashed_base() {
        let base = BaseUrl::parse("http://127.0.0.1:9000/").unwrap();
        assert_eq!(
            base.short_url("alice.test", "x"),
            "http://127.0.0.1:9000/@alice.test/x"
        );
    }

    #[test]
    fn test_is_loopback() {
        assert!(BaseUrl::parse("http://localhost:9000")
            .unwrap()
            .is_loopback());
        assert!(BaseUrl::parse("http://127.0.0.1:9000")
            .unwrap()
            .is_loopback());
        assert!(!BaseUrl::parse("https://atpr.to").unwrap().is_loopback());
        // A prefix check would have called both of these loopback.
        assert!(!BaseUrl::parse("http://localhost.evil.com")
            .unwrap()
            .is_loopback());
        assert!(!BaseUrl::parse("http://127.0.0.1.evil.com")
            .unwrap()
            .is_loopback());
    }

    #[test]
    fn test_base_url_rejects_garbage() {
        assert!(BaseUrl::parse("not a url").is_err());
    }

    #[test]
    fn test_session_store_empty_string_is_memory() {
        let cfg: Config = toml::from_str(r#"session_file = """#).unwrap();
        assert_eq!(cfg.session_store, SessionStore::Memory);
    }

    #[test]
    fn test_session_store_path_is_file() {
        let cfg: Config = toml::from_str(r#"session_file = "/tmp/s.json""#).unwrap();
        assert_eq!(
            cfg.session_store,
            SessionStore::File(PathBuf::from("/tmp/s.json"))
        );
    }

    #[test]
    fn test_session_store_dynamodb_url_is_a_table() {
        let cfg: Config = toml::from_str(r#"session_file = "dynamodb://atpr-to-sessions""#).unwrap();
        assert_eq!(
            cfg.session_store,
            SessionStore::Dynamo("atpr-to-sessions".to_string())
        );
        assert_eq!(cfg.session_store.table(), Some("atpr-to-sessions"));
        assert_eq!(cfg.session_store.path(), None);
    }

    /// A scheme with no table names nothing. Left to fall through it would
    /// become `File("dynamodb://")`, create that file, and look like it worked
    /// right up until a second Lambda instance answered a request.
    #[test]
    fn test_session_store_dynamodb_without_a_table_is_rejected() {
        let err = toml::from_str::<Config>(r#"session_file = "dynamodb://""#)
            .expect_err("a scheme with no table must not load");
        assert!(
            err.to_string().contains("table name"),
            "the error should say what is missing: {err}"
        );
    }

    /// A path is still a path, including one that merely mentions the word.
    #[test]
    fn test_a_path_is_not_mistaken_for_a_table() {
        let cfg: Config = toml::from_str(r#"session_file = "/var/dynamodb/sessions.json""#).unwrap();
        assert_eq!(
            cfg.session_store,
            SessionStore::File(PathBuf::from("/var/dynamodb/sessions.json"))
        );
    }

    /// Bug #8: a zero rate limit used to panic on every cold start via
    /// `GovernorConfigBuilder::finish().unwrap()`. It is now rejected at parse
    /// time, with a message, before the process starts serving.
    #[test]
    fn test_zero_rate_limit_is_rejected() {
        assert!(
            toml::from_str::<Config>("[rate_limit]\nper_second = 0\nburst_size = 10").is_err(),
            "zero per_second must not deserialize"
        );
    }

    #[test]
    fn test_zero_burst_size_is_rejected() {
        assert!(toml::from_str::<Config>("[rate_limit]\nper_second = 2\nburst_size = 0").is_err());
    }

    #[test]
    fn test_malformed_base_url_is_rejected() {
        assert!(toml::from_str::<Config>(r#"base_url = "://nope""#).is_err());
    }

    /// A typo'd key is a configuration error, not something to ignore.
    #[test]
    fn test_unknown_key_is_rejected() {
        assert!(toml::from_str::<Config>(r#"base_rul = "https://atpr.to""#).is_err());
    }

    #[test]
    fn test_partial_config_fills_from_defaults() {
        let cfg: Config = toml::from_str(r#"base_url = "https://example.com""#).unwrap();
        assert_eq!(cfg.base_url.as_str(), "https://example.com");
        assert_eq!(cfg.rate_limit.per_second.get(), 2);
        assert_eq!(cfg.static_cache_max_age, 15);
    }
}
