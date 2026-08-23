//! Identity resolution and the shared outbound HTTP client.
//!
//! This is the one place in `atpr-core` that touches jacquard, and only its
//! identity-resolution surface (handle → DID → DID document) — the same
//! confinement rule that keeps jacquard out of handlers, applied to the read
//! path. The OAuth and repo-write surfaces stay in `atpr-server`.

use jacquard::identity::resolver::ResolverOptions;
use jacquard::identity::JacquardResolver;

use crate::config::Config;

/// The identity resolver used by the direct resolution path.
pub type Resolver = JacquardResolver<reqwest::Client>;

/// Build the shared outbound HTTP client.
///
/// Every outbound call goes through this one client, so the timeouts apply
/// everywhere. Without them the only bound on a hung upstream is whatever sits
/// in front of the server — Lambda's 30s function timeout, or a reverse proxy.
pub fn http_client(config: &Config) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(
            config.http_timeout_ms.get(),
        ))
        .connect_timeout(std::time::Duration::from_millis(
            config.http_connect_timeout_ms.get(),
        ))
        .user_agent(concat!("atpr.to/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("failed to build HTTP client")
}

/// Build the identity resolver over the shared HTTP client.
///
/// Mirrors `JacquardResolver::default()` but reuses our client instead of
/// spawning a fresh untimed one.
pub fn identity_resolver(http: reqwest::Client) -> Resolver {
    JacquardResolver::new(http, ResolverOptions::default())
        .with_system_dns()
        .with_cache()
}
