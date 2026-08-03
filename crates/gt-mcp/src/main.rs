//! The `gt-mcp` binary: install logging, then serve over stdio.
//!
//! stdout carries the JSON-RPC stream. All logging goes to stderr -- a stray
//! write to stdout corrupts the protocol.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gt_mcp=info".into()),
        )
        .init();

    tracing::info!("gt-mcp starting");
    Ok(())
}
