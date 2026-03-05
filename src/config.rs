use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub base_url: String,
    pub slingshot_url: String,
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

#[derive(Debug, Deserialize, Clone)]
pub struct RateLimitConfig {
    pub per_second: u64,
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

/// Loads config: compiled defaults → Config.toml (optional) → ATPR_ env vars.
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
                .separator("__")
                .try_parsing(true),
        )
        .build()
        .and_then(|c| c.try_deserialize::<Config>());

    match result {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::warn!("Config load error: {e}, using defaults");
            Config::default()
        }
    }
}
