//! The MCP server: a tool router plus a hand-written `ServerHandler`.
//!
//! The handler impl is explicit rather than macro-generated because the
//! resource and prompt surfaces live in the same block as the tools.

use crate::tools::backward_induction::SolveBackwardInduction;
use crate::tools::convert::ConvertForm;
use crate::tools::dominance::SolveDominance;
use crate::tools::mixed_nash::SolveMixedNash;
use crate::tools::pure_nash::SolvePureNash;
use crate::tools::repeated::AnalyzeRepeatedGame;
use crate::tools::structure::AnalyzePayoffStructure;
use crate::tools::validate::ValidateGame;
use crate::tools::verify::VerifyEquilibrium;
use rmcp::handler::server::router::tool::{SyncTool, ToolBase, ToolRouter};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, GetPromptRequestParams, GetPromptResponse, Implementation, ListPromptsResult,
    ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse,
    ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{tool_handler, ErrorData, RoleServer, ServerHandler};

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

    /// Listed in the order the design lays them out -- foundation, conversion,
    /// solvers, verification, analysis. `ToolRouter` keys tools by name and
    /// `list_all` returns them sorted, so this order is for the reader here,
    /// not something a host sees.
    pub fn tool_router() -> ToolRouter<Self> {
        ToolRouter::new()
            .with_route((tool_attr::<ValidateGame>(), invoke_sync::<ValidateGame>))
            .with_route((tool_attr::<ConvertForm>(), invoke_sync::<ConvertForm>))
            .with_route((tool_attr::<SolveDominance>(), invoke_sync::<SolveDominance>))
            .with_route((tool_attr::<SolvePureNash>(), invoke_sync::<SolvePureNash>))
            .with_route((tool_attr::<SolveMixedNash>(), invoke_sync::<SolveMixedNash>))
            .with_route((
                tool_attr::<SolveBackwardInduction>(),
                invoke_sync::<SolveBackwardInduction>,
            ))
            .with_route((
                tool_attr::<VerifyEquilibrium>(),
                invoke_sync::<VerifyEquilibrium>,
            ))
            .with_route((
                tool_attr::<AnalyzePayoffStructure>(),
                invoke_sync::<AnalyzePayoffStructure>,
            ))
            .with_route((
                tool_attr::<AnalyzeRepeatedGame>(),
                invoke_sync::<AnalyzeRepeatedGame>,
            ))
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
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
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

    /// The concept resources. Pagination is not used: six entries fit in one
    /// page, so `next_cursor` stays absent and the cursor in `request` is
    /// never consulted.
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(crate::resources::list()))
    }

    /// Unlike a tool call, an unknown resource is a protocol-level error: a
    /// URI either names a concept or it does not, and there is no useful
    /// structured payload to hand back for one that does not.
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        crate::resources::read(&request.uri).map(Into::into)
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        Ok(ListPromptsResult::with_all_items(crate::prompts::list()))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, ErrorData> {
        crate::prompts::get(&request.name, request.arguments.as_ref()).map(Into::into)
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

    /// A capability left unadvertised is a surface no host will ever probe,
    /// however completely it is implemented behind the handler.
    #[test]
    fn the_server_advertises_tools_resources_and_prompts() {
        let caps = GtServer::new().get_info().capabilities;
        assert!(caps.tools.is_some(), "tools not advertised");
        assert!(caps.resources.is_some(), "resources not advertised");
        assert!(caps.prompts.is_some(), "prompts not advertised");
    }

    /// The whole v1.0 tool surface. A tool written but never routed is
    /// invisible to every host, and nothing else in the crate would notice.
    /// Compared as a sorted set because `list_all` sorts by name.
    #[test]
    fn the_router_registers_the_whole_v1_surface() {
        let mut names: Vec<String> = GtServer::tool_router()
            .list_all()
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "analyze_payoff_structure",
                "analyze_repeated_game",
                "convert_form",
                "solve_backward_induction",
                "solve_dominance",
                "solve_mixed_nash",
                "solve_pure_nash",
                "validate_game",
                "verify_equilibrium",
            ]
        );
    }

    /// A tool with no description or no schemas is one the host LLM cannot
    /// choose correctly.
    #[test]
    fn every_registered_tool_is_described_and_schematized() {
        for tool in GtServer::tool_router().list_all() {
            let name = &tool.name;
            assert!(
                tool.description.as_ref().is_some_and(|d| !d.is_empty()),
                "{name} has no description"
            );
            assert!(!tool.input_schema.is_empty(), "{name} has no input schema");
            let out = tool
                .output_schema
                .as_ref()
                .unwrap_or_else(|| panic!("{name} publishes no output schema"));
            // Every tool declares Output = CallToolResult so it can set
            // isError, then republishes its payload's schema. If that override
            // were forgotten the host would be handed CallToolResult's shape,
            // which says nothing about the answer.
            let text = serde_json::to_string(&**out).unwrap();
            assert!(
                text.contains("\"ok\""),
                "{name} publishes CallToolResult's schema, not its payload's"
            );
        }
    }
}
