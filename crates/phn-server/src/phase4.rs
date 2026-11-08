//! Phase 4 tools: public data and evidence platform.

use phn_core::Envelope;
use phn_tools::chronic::{PlacesArgs, PlacesResult};
use phn_tools::ebp::{CdcGuidanceArgs, CdcGuidanceResult, PubmedResult, SearchPubmedArgs};
use phn_tools::epi::{ChoroplethResult, CreateChoroplethArgs};
use phn_tools::equity::{
    DisparityArgs, DisparityResult, LanguageAccessArgs, LanguageAccessResult, ProfileArgs,
    ProfileResult, SviArgs, SviResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase4_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "build_community_health_profile",
        description = "One-call community health profile for any Massachusetts city or town, \
            county, or census tract: population, age, race and ethnicity, language and limited \
            English proficiency, income and poverty, insurance, disability, housing cost burden \
            and crowding, vehicles, education (Census ACS 5-year, each figure with its table ID \
            and margin of error, unreliable estimates flagged), CDC PLACES chronic disease \
            estimates, and the Social Vulnerability Index, compared with Massachusetts and the \
            U.S. Works offline from bundled snapshots.",
        annotations(title = "Build a community health profile", read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<ProfileResult>>()
    )]
    async fn build_community_health_profile(
        &self,
        Parameters(args): Parameters<ProfileArgs>,
    ) -> CallToolResult {
        respond(phn_tools::equity::build_community_health_profile(&self.ctx, args).await)
    }

    #[tool(
        name = "get_social_vulnerability_index",
        description = "CDC/ATSDR Social Vulnerability Index (SVI 2022) percentile ranks for the \
            census tracts in a Massachusetts town, county, or a single tract: overall and the \
            four themes (socioeconomic status, household characteristics, racial and ethnic \
            minority status, housing type and transportation), ranked within Massachusetts or \
            nationally, with the most vulnerable tracts listed first.",
        annotations(title = "Social Vulnerability Index", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<SviResult>>()
    )]
    async fn get_social_vulnerability_index(
        &self,
        Parameters(args): Parameters<SviArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::equity::get_social_vulnerability_index(ctx, args)
        })
        .await
    }

    #[tool(
        name = "language_access_profile",
        description = "Limited English proficiency (LEP) by language group for a Massachusetts \
            town, county, or tract (Census ACS: speak English less than \"very well\"), with \
            margins of error, each group compared with the HHS LEP guidance safe harbor for \
            written translation (5% or 1,000 persons, whichever is less), limited-English \
            households, and detailed languages for the surrounding area and the state. \
            Decision support, not legal advice.",
        annotations(title = "Language access profile", read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<LanguageAccessResult>>()
    )]
    async fn language_access_profile(
        &self,
        Parameters(args): Parameters<LanguageAccessArgs>,
    ) -> CallToolResult {
        respond(phn_tools::equity::language_access_profile(&self.ctx, args).await)
    }

    #[tool(
        name = "measure_health_disparities",
        description = "Measure health disparities across groups (race and ethnicity, towns, \
            income levels): rates with exact confidence intervals, rate ratios and rate \
            differences versus a reference group, the index of disparity, between-group variance, \
            and excess events and the population attributable fraction. Takes counts and \
            populations inline or from an aggregated CSV/XLSX in /data (small counts suppressed).",
        annotations(title = "Measure health disparities", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<DisparityResult>>()
    )]
    async fn measure_health_disparities(
        &self,
        Parameters(args): Parameters<DisparityArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::equity::measure_health_disparities(ctx, args)
        })
        .await
    }

    #[tool(
        name = "get_places_estimates",
        description = "CDC PLACES model-based estimates of chronic disease, risk behaviors, \
            prevention, disability, and health-related social needs among adults for a \
            Massachusetts county, city or town, census tract, or ZIP code (ZCTA), with 95% \
            confidence intervals, the BRFSS data year, and comparisons with Massachusetts and \
            the U.S. Choose measures or a category; works offline from a bundled snapshot.",
        annotations(title = "CDC PLACES estimates", read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<PlacesResult>>()
    )]
    async fn get_places_estimates(
        &self,
        Parameters(args): Parameters<PlacesArgs>,
    ) -> CallToolResult {
        respond(phn_tools::chronic::get_places_estimates(&self.ctx, args).await)
    }

    #[tool(
        name = "create_choropleth_map",
        description = "Draw a choropleth map of Massachusetts cities and towns, census tracts, \
            ZIP code areas (ZCTAs), or school districts from aggregated values given inline or in \
            a CSV/XLSX in /data. Natural-breaks or quantile classes (3-7) in an accessible blue \
            ramp, a legend, hatched suppressed or missing areas, and a source note; saves SVG and \
            PNG to the output folder and returns alt text and the class table. Record-level \
            files are refused.",
        annotations(title = "Create a choropleth map", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ChoroplethResult>>()
    )]
    async fn create_choropleth_map(
        &self,
        Parameters(args): Parameters<CreateChoroplethArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epi::create_choropleth_map(ctx, args)
        })
        .await
    }

    #[tool(
        name = "search_pubmed",
        description = "Search PubMed for evidence: build the query from PICO elements (MeSH \
            headings checked against MeSH, plus free-text synonyms) or take a ready-made query, \
            with filters for systematic reviews, meta-analyses, randomized trials, guidelines, \
            years, language, and humans. Returns PMID, title, journal, year, authors, an \
            abstract excerpt, DOI, links, and the exact query string to rerun.",
        annotations(title = "Search PubMed", read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<PubmedResult>>()
    )]
    async fn search_pubmed(
        &self,
        Parameters(args): Parameters<SearchPubmedArgs>,
    ) -> CallToolResult {
        respond(phn_tools::ebp::search_pubmed(&self.ctx, args).await)
    }

    #[tool(
        name = "search_cdc_guidance",
        description = "Find current CDC guidance on a topic, newest first: MMWR reports (CDC \
            content syndication API) and Health Alert Network (HAN) advisories, alerts, and \
            updates, each with title, date, series, link, and CDC's summary, plus a CDC Stacks \
            search link for guidelines and archived pages. States how each series was searched \
            and whether it could be reached.",
        annotations(title = "Search CDC guidance", read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<CdcGuidanceResult>>()
    )]
    async fn search_cdc_guidance(
        &self,
        Parameters(args): Parameters<CdcGuidanceArgs>,
    ) -> CallToolResult {
        respond(phn_tools::ebp::search_cdc_guidance(&self.ctx, args).await)
    }
}
