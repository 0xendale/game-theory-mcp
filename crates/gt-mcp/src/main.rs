//! The `gt-mcp` binary: install logging, then serve over stdio.
//!
//! stdout carries the JSON-RPC stream. All logging goes to stderr -- a stray
//! write to stdout corrupts the protocol.

use gt_mcp::server::GtServer;
use rmcp::transport::io::stdio;
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gt_mcp=info".into()),
        )
        .init();

    tracing::info!("gt-mcp listening on stdio");
    let service = GtServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
