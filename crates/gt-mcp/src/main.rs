//! MCP adapter for `gt-core`. Tool registration, wire types, and error
//! mapping only -- every number in a response comes from `gt-core`.
//!
//! stdout carries the JSON-RPC stream. All logging goes to stderr.

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
