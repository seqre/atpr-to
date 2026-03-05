use serde::Deserialize;

/// Application configuration, loaded from defaults, `Config.toml`, and env vars.
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// Base URL for short links and OAuth metadata (e.g. `https://atpr.to`).
    pub base_url: String,
    /// Slingshot relay URL used for fast AT Protocol resolution.
    pub slingshot_url: String,
    /// Rate limiting parameters for mutation routes.
    pub rate_limit: RateLimitConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "https://atpr.to".to_string(),
            slingshot_url: "https://slingshot.microcosm.blue/".to_string(),
            rate_limit: RateLimitConfig::default(),
        }
    }
}

/// Rate limiting configuration for mutation routes.
#[derive(Debug, Deserialize, Clone)]
pub struct RateLimitConfig {
    /// Sustained request rate in requests per second.
    pub per_second: u64,
    /// Maximum burst above the sustained rate.
    pub burst_size: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            per_second: 2,
            burst_size: 10,
        }
    }
}

/// Loads config: compiled defaults → Config.toml (optional) → `ATPR__` env vars.
/// Falls back to defaults on any error (logs warning).
pub fn load() -> Config {
    let result = config::Config::builder()
        .set_default("base_url", "https://atpr.to")
        .unwrap()
        .set_default("slingshot_url", "https://slingshot.microcosm.blue/")
        .unwrap()
        .set_default("rate_limit.per_second", 2u64)
        .unwrap()
        .set_default("rate_limit.burst_size", 10u32)
        .unwrap()
        .add_source(config::File::with_name("Config").required(false))
        .add_source(
            config::Environment::with_prefix("ATPR")
                .prefix_separator("__")
                .separator("__")
                .try_parsing(true),
        )
        .build()
        .and_then(|c| c.try_deserialize::<Config>());

    match result {
        Ok(cfg) => cfg,
        // coverage:excl-start
        Err(e) => {
            tracing::warn!("Config load error: {e}, using defaults");
            Config::default()
        }
        // coverage:excl-stop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let c = Config::default();
        assert_eq!(c.base_url, "https://atpr.to");
        assert_eq!(c.slingshot_url, "https://slingshot.microcosm.blue/");
    }

    #[test]
    fn test_rate_limit_defaults() {
        let r = RateLimitConfig::default();
        assert_eq!(r.per_second, 2);
        assert_eq!(r.burst_size, 10);
    }

    #[test]
    fn test_load_returns_valid_config() {
        let c = load();
        assert!(!c.base_url.is_empty());
        assert!(!c.slingshot_url.is_empty());
        assert!(c.rate_limit.per_second > 0);
        assert!(c.rate_limit.burst_size > 0);
    }
}
