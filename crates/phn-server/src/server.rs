//! The MCP server: tool definitions, toolset filtering, and resources.

use std::sync::Arc;

use phn_core::Envelope;
use phn_tools::Context;
use phn_tools::cd::{LookupReportableArgs, ReportableLookup};
use phn_tools::core::{
    ConfigureJurisdictionArgs, ConfigureResult, DataSourceStatus, DataSourceStatusArgs,
};
use phn_tools::epi::{CalculateRatesArgs, RatesResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListResourcesResult, ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams,
    ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ServerCapabilities,
    ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, Peer, RoleServer, ServerHandler, tool, tool_router};
use serde::Serialize;

pub(crate) const INSTRUCTIONS: &str = "Tools for Massachusetts public health nurses (local \
boards of health, school districts, and DPH). Every tool returns a markdown summary plus \
structured data with sources, caveats, and a privacy block; cite the sources when you answer. \
The nurse's files are in the mounted /data folder. Tools that read local data return aggregates \
by default and apply small-cell suppression; never ask for row-level data unless the nurse needs \
it. Anything identifiable (letters, rosters, de-identified copies) is written to /data/output and \
is not returned. Clinical and legal answers are decision support: the nurse decides. If the \
nurse's town is not configured, offer configure_jurisdiction.";

/// The PHN MCP server.
#[derive(Clone)]
pub struct PhnServer {
    pub(crate) ctx: Arc<Context>,
    tool_router: ToolRouter<Self>,
}

/// Turn a tool result into an MCP result: markdown first, then the full
/// envelope as JSON (also sent as structured content).
pub(crate) fn respond<T: Serialize>(result: phn_core::Result<Envelope<T>>) -> CallToolResult {
    match result {
        Ok(env) => {
            let markdown = env.to_markdown();
            match serde_json::to_value(&env) {
                Ok(json) => {
                    let mut r = CallToolResult::success(vec![
                        ContentBlock::text(markdown),
                        ContentBlock::text(json.to_string()),
                    ]);
                    r.structured_content = Some(json);
                    r
                }
                Err(e) => CallToolResult::error(vec![ContentBlock::text(format!(
                    "internal error serializing the result: {e}"
                ))]),
            }
        }
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e.to_string())]),
    }
}

/// Run blocking work (file parsing, indexing) off the async runtime.
pub(crate) async fn blocking<T, F>(ctx: &Arc<Context>, f: F) -> CallToolResult
where
    T: Serialize + Send + 'static,
    F: FnOnce(&Context) -> phn_core::Result<Envelope<T>> + Send + 'static,
{
    let ctx = Arc::clone(ctx);
    match tokio::task::spawn_blocking(move || f(&ctx)).await {
        Ok(r) => respond(r),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(format!(
            "internal error: the tool task failed ({e})"
        ))]),
    }
}

#[tool_router]
impl PhnServer {
    /// Create the server around a shared context.
    pub fn new(ctx: Arc<Context>) -> Self {
        let mut tool_router = Self::tool_router();
        tool_router.merge(Self::phase1_router());
        Self { ctx, tool_router }
    }

    /// Every tool definition, regardless of toolset (for tests and docs).
    pub fn all_tools(&self) -> Vec<Tool> {
        self.tool_router.list_all()
    }

    /// Tool definitions enabled by the current toolsets.
    pub fn enabled_tools(&self) -> Vec<Tool> {
        let enabled = self.ctx.enabled_toolsets();
        self.tool_router
            .list_all()
            .into_iter()
            .filter(|t| {
                phn_tools::registry::tool(&t.name).is_some_and(|m| enabled.contains(&m.toolset))
            })
            .collect()
    }

    // ----------------------------------------------------------------- Phase 0

    #[tool(
        name = "get_data_source_status",
        description = "List every data source the server uses, with its data date, review status \
            of bundled content, whether the live endpoint is reachable, and known caveats (for \
            example federal datasets changed since 2025). Use when data looks stale, the network \
            seems blocked, or the nurse asks where answers come from.",
        annotations(title = "Data source status", read_only_hint = true, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<DataSourceStatus>>()
    )]
    async fn get_data_source_status(
        &self,
        Parameters(args): Parameters<DataSourceStatusArgs>,
    ) -> CallToolResult {
        respond(phn_tools::core::get_data_source_status(&self.ctx, args).await)
    }

    #[tool(
        name = "configure_jurisdiction",
        description = "View or set the nurse's Massachusetts town (county and HMCC region are \
            derived), school district, board of health phone, small-cell suppression threshold, \
            preferred languages, role profile or toolsets, and document letterhead. Call with no \
            arguments to show current settings. Settings persist in /cache.",
        annotations(
            title = "Configure jurisdiction",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<ConfigureResult>>()
    )]
    async fn configure_jurisdiction(
        &self,
        Parameters(args): Parameters<ConfigureJurisdictionArgs>,
        peer: Peer<RoleServer>,
    ) -> CallToolResult {
        let result = phn_tools::core::configure_jurisdiction(&self.ctx, args);
        if result.as_ref().is_ok_and(|e| e.data.toolsets_changed) {
            if let Err(e) = peer.notify_tool_list_changed().await {
                tracing::warn!(error = ?e, "could not send tools/list_changed");
            }
        }
        respond(result)
    }

    #[tool(
        name = "calculate_rates",
        description = "Calculate crude and category-specific rates (per 1,000, 10,000, or \
            100,000) from counts and populations, with exact Poisson confidence intervals, a \
            crude total, and flags for unreliable (<20 events) and suppressible counts.",
        annotations(
            title = "Calculate rates",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<RatesResult>>()
    )]
    async fn calculate_rates(
        &self,
        Parameters(args): Parameters<CalculateRatesArgs>,
    ) -> CallToolResult {
        respond(phn_tools::epi::calculate_rates(&self.ctx, args))
    }

    #[tool(
        name = "lookup_reportable_condition",
        description = "For a disease, organism, or lab result, say whether it is reportable in \
            Massachusetts under 105 CMR 300.000, who reports it (provider, lab, board of health), \
            how fast (immediately by phone or within 24 hours), to whom, and which chapter of \
            DPH's Guide to Surveillance, Reporting and Control applies.",
        annotations(
            title = "Look up a reportable condition",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<ReportableLookup>>()
    )]
    async fn lookup_reportable_condition(
        &self,
        Parameters(args): Parameters<LookupReportableArgs>,
    ) -> CallToolResult {
        respond(phn_tools::cd::lookup_reportable_condition(&self.ctx, args))
    }
}

impl ServerHandler for PhnServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .enable_resources()
                .build(),
        )
        .with_server_info(
            Implementation::new(phn_core::SERVER_NAME, phn_core::VERSION)
                .with_title("Public Health Nursing MCP Server")
                .with_website_url("https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server"),
        )
        .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.enabled_tools()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tool_router.get(name).cloned()
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.to_string();
        if let Some(meta) = phn_tools::registry::tool(&name) {
            if !self.ctx.enabled_toolsets().contains(&meta.toolset) {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "The tool '{name}' belongs to the '{}' toolset, which is not enabled. \
                     Enable it with configure_jurisdiction (toolsets) or PHN_TOOLSETS.",
                    meta.toolset
                ))])
                .into());
            }
        }
        let tcc = ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(crate::resources::list(
            &self.ctx,
        )))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        match crate::resources::read(&self.ctx, &request.uri) {
            Some((text, mime)) => Ok(ReadResourceResult::new(vec![
                ResourceContents::text(text, request.uri.clone()).with_mime_type(mime),
            ])
            .into()),
            None => Err(ErrorData::resource_not_found(
                format!("unknown resource {}", request.uri),
                None,
            )),
        }
    }
}

/// Build a resource descriptor.
pub(crate) fn resource(
    uri: &str,
    name: &str,
    title: &str,
    description: &str,
    mime: &str,
) -> Resource {
    Resource::new(uri, name)
        .with_title(title)
        .with_description(description)
        .with_mime_type(mime)
}
