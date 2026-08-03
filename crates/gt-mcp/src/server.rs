//! The MCP server: a tool router plus a hand-written `ServerHandler`.
//!
//! The handler impl is explicit rather than macro-generated because later
//! increments add `read_resource` and `list_prompts` to this same block.

use crate::tools::validate::ValidateGame;
use rmcp::handler::server::router::tool::{SyncTool, ToolBase, ToolRouter};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Implementation, ServerCapabilities, ServerInfo, Tool};
use rmcp::{tool_handler, ErrorData, ServerHandler};

/// Build a tool's advertised attributes from its [`ToolBase`].
///
/// Hand-rolled because rmcp's equivalent is crate-private.
fn tool_attr<T: ToolBase>() -> Tool {
    let mut tool = Tool::new(
        T::name(),
        T::description().unwrap_or_default(),
        T::input_schema().expect("every gt-mcp tool declares an input schema"),
    );
    // Tool is #[non_exhaustive]: assigning a field is allowed, building it
    // with a struct expression is not.
    tool.output_schema = T::output_schema();
    tool
}

/// Invoke a tool whose output is already a [`CallToolResult`], passing it
/// through untouched.
///
/// Deliberately not [`ToolRouter::with_sync_tool`]. That helper routes the
/// output through `Json`, and `Json<T>: IntoContents` becomes
/// `CallToolResult::success(..)` -- so a `CallToolResult` output would be
/// nested inside a second one, and the outer `isError` would be hard-coded
/// false no matter what the tool decided. `CallToolResult` implements
/// `IntoCallToolResult` by passing itself through, so routing it directly
/// keeps `isError` under the tool's control.
fn invoke_sync<T>(
    service: &GtServer,
    Parameters(param): Parameters<T::Parameter>,
) -> Result<CallToolResult, ErrorData>
where
    T: SyncTool<GtServer> + ToolBase<Output = CallToolResult>,
{
    T::invoke(service, param).map_err(Into::into)
}

#[derive(Clone)]
pub struct GtServer {
    tool_router: ToolRouter<Self>,
}

impl GtServer {
    pub fn new() -> Self {
        GtServer {
            tool_router: Self::tool_router(),
        }
    }

    pub fn tool_router() -> ToolRouter<Self> {
        ToolRouter::new().with_route((tool_attr::<ValidateGame>(), invoke_sync::<ValidateGame>))
    }
}

impl Default for GtServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for GtServer {
    fn get_info(&self) -> ServerInfo {
        // ServerInfo is #[non_exhaustive], so it is built through its
        // constructor rather than a struct expression.
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            // Not Implementation::from_build_env(): its env! expands inside
            // rmcp, so the server would announce itself as "rmcp".
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Exact game-theoretic computation. Formalize the scenario as a \
                 game, check it with validate_game, then analyse it. Every \
                 answer is exact -- probabilities and payoffs come back as \
                 fractions, with a decimal alongside for display only.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_announces_itself_not_rmcp() {
        // Implementation::from_build_env() reads rmcp's own CARGO_* vars, so
        // the handshake would otherwise say name "rmcp", version "3.1.0".
        let info = GtServer::new().get_info();
        assert_eq!(info.server_info.name, "gt-mcp");
        assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn the_server_advertises_tool_capability() {
        assert!(GtServer::new().get_info().capabilities.tools.is_some());
    }
}
