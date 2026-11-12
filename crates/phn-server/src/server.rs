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
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, GetPromptRequestParams,
    GetPromptResponse, Implementation, ListPromptsResult, ListResourcesResult, ListToolsResult,
    PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult,
    Resource, ResourceContents, ServerCapabilities, ServerConfig, Tool,
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

/// The tool-level error for a tool whose toolset is off. When an environment
/// variable pins the toolsets, `configure_jurisdiction` cannot enable the tool,
/// so the message names the variable and points at the client configuration.
pub(crate) fn disabled_tool_message(
    name: &str,
    toolset: phn_core::Toolset,
    config: &phn_core::Config,
) -> String {
    let pins: Vec<&str> = config
        .env_overrides()
        .into_iter()
        .filter(|v| matches!(*v, "PHN_PROFILE" | "PHN_TOOLSETS"))
        .collect();
    if pins.is_empty() {
        format!(
            "The tool '{name}' belongs to the '{toolset}' toolset, which is not enabled. \
             Enable it with configure_jurisdiction (toolsets) or PHN_TOOLSETS."
        )
    } else {
        format!(
            "The tool '{name}' belongs to the '{toolset}' toolset, which is not enabled. The \
             enabled toolsets are fixed by {} in the MCP client configuration (the server's \
             docker arguments), so configure_jurisdiction cannot enable it. To use this tool, \
             ask the nurse to edit the client configuration (add '{toolset}' to PHN_TOOLSETS, \
             or change or remove PHN_PROFILE) and restart the client.",
            pins.join(" and ")
        )
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
        tool_router.merge(Self::phase2_router());
        tool_router.merge(Self::phase3_router());
        tool_router.merge(Self::phase4_router());
        tool_router.merge(Self::phase5_router());
        tool_router.merge(Self::phase6_router());
        tool_router.merge(Self::phase7_router());
        tool_router.merge(Self::phase8_router());
        tool_router.merge(Self::phase9_router());
        tool_router.merge(Self::phase10_router());
        tool_router.merge(Self::phase11_router());
        tool_router.merge(Self::phase12_router());
        tool_router.merge(Self::phase13_ics_router());
        tool_router.merge(Self::phase13_coop_router());
        tool_router.merge(Self::phase13_hseep_router());
        tool_router.merge(Self::phase13_cerc_router());
        tool_router.merge(Self::phase14_water_router());
        tool_router.merge(Self::phase14_ejclimate_router());
        tool_router.merge(Self::phase14_vectorhousing_router());
        tool_router.merge(Self::phase14_chemfood_router());
        tool_router.merge(Self::phase15_screen_router());
        tool_router.merge(Self::phase15_prevent_router());
        tool_router.merge(Self::phase15_cha_router());
        tool_router.merge(Self::phase15_geo_router());
        tool_router.merge(Self::phase16_design_router());
        tool_router.merge(Self::phase16_interventions_router());
        tool_router.merge(Self::phase16_appraise_router());
        tool_router.merge(Self::phase16_qi_router());
        tool_router.merge(Self::phase17_grants_router());
        tool_router.merge(Self::phase17_budget_router());
        tool_router.merge(Self::phase17_finance_router());
        tool_router.merge(Self::phase18_strategy_router());
        tool_router.merge(Self::phase18_workforce_router());
        tool_router.merge(Self::phase18_ethics_router());
        tool_router.merge(Self::phase19_tracking_router());
        tool_router.merge(Self::phase19_law_router());
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
            preferred languages, role profile or toolsets, document letterhead, and the vaccine \
            recommendation basis (DPH, professional societies, or CDC/ACIP first). Call with no \
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
        if result.as_ref().is_ok_and(|e| e.data.toolsets_changed)
            && let Err(e) = peer.notify_tool_list_changed().await
        {
            tracing::warn!(error = ?e, "could not send tools/list_changed");
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
                .enable_prompts()
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
        if let Some(meta) = phn_tools::registry::tool(&name)
            && !self.ctx.enabled_toolsets().contains(&meta.toolset)
        {
            return Ok(
                CallToolResult::error(vec![ContentBlock::text(disabled_tool_message(
                    &name,
                    meta.toolset,
                    &self.ctx.config,
                ))])
                .into(),
            );
        }
        let tcc = ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
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
        crate::prompts::get(&request.name, &request.arguments).map(Into::into)
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

#[cfg(test)]
mod tests {
    use super::disabled_tool_message;
    use phn_core::{Config, Profile, Toolset};
    use std::path::Path;

    #[test]
    fn disabled_tool_message_names_the_pinning_variable() {
        let mut c = Config::for_root(Path::new("/unused"));
        let free = disabled_tool_message("steadi_fall_risk_assessment", Toolset::Aging, &c);
        assert!(free.contains("'aging' toolset, which is not enabled"));
        assert!(free.contains("Enable it with configure_jurisdiction (toolsets)"));

        // With PHN_PROFILE pinned (the README's configuration), saving toolsets
        // cannot work, so the message must not suggest it.
        c.env_profile = Some(Profile::CdNurse);
        let pinned = disabled_tool_message("steadi_fall_risk_assessment", Toolset::Aging, &c);
        assert!(pinned.contains("not enabled"));
        assert!(pinned.contains("fixed by PHN_PROFILE in the MCP client configuration"));
        assert!(pinned.contains("configure_jurisdiction cannot enable it"));
        assert!(!pinned.contains("Enable it with configure_jurisdiction"));

        c.env_toolsets = Some([Toolset::Cd].into());
        let both = disabled_tool_message("steadi_fall_risk_assessment", Toolset::Aging, &c);
        assert!(both.contains("fixed by PHN_PROFILE and PHN_TOOLSETS"));
    }
}
