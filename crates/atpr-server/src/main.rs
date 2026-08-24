//! Entry point.
//!
//! Two runtimes behind one switch. On Lambda the router is handed to the
//! `provided.al2023` runtime; locally it is served by `axum::serve` on a TCP
//! port, so developing against it needs nothing but `cargo run`.
//!
//! The switch is `AWS_LAMBDA_FUNCTION_NAME`, which the Lambda runtime sets and
//! nothing else does — so neither mode needs to be selected by hand, and there
//! is no flag to forget.

use std::net::SocketAddr;

use lambda_http::{run, tracing, Error};

/// Port the local server binds when `ATPR_PORT` is unset.
const DEFAULT_LOCAL_PORT: u16 = 9000;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let router = atpr_server::router().await;

    if std::env::var_os("AWS_LAMBDA_FUNCTION_NAME").is_some() {
        return run(router).await;
    }

    let port = std::env::var("ATPR_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_LOCAL_PORT);
    // Loopback only. This is a development server with no TLS, and the session
    // cookie drops its `Secure` attribute when `base_url` is loopback — binding
    // it to every interface would put that cookie on the network in the clear.
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    ::tracing::info!(%addr, "listening");
    axum::serve(listener, router).await?;
    Ok(())
}
