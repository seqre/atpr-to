//! Standalone redirect server for atpr.to short links.
//!
//! Serves exactly the public read path from `atpr-core` — `GET
//! /@{handle}/{code}` plus a `/health` probe — as a plain HTTP server, so a
//! self-hoster can resolve atpr.to-style links on their own domain without any
//! of the OAuth/PDS-write machinery. Put it behind a reverse proxy that
//! terminates TLS and overwrites `X-Forwarded-For`.
//!
//! Configuration is the same chain as the main server: compiled defaults →
//! `Config.toml` → `ATPR__` env vars. The address to bind comes from
//! `ATPR__BIND_ADDR` (default `127.0.0.1:8080`); loopback by default because
//! there is no TLS here.

use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = atpr_core::config::load();
    let addr: SocketAddr = config
        .bind_addr
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid ATPR__BIND_ADDR {}: {e}", config.bind_addr))?;

    let http = atpr_core::identity::http_client(&config);
    let state = std::sync::Arc::new(atpr_core::redirect::ResolveState::new(config, http));
    let app = atpr_core::redirect::router_with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, app).await?;
    Ok(())
}
