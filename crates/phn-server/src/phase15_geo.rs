//! Phase 15 tools (Offline geocoder): EPI-13.

use phn_core::Envelope;
use phn_tools::epi::{GeocodeArgs, GeocodeResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking};

// Phase 15 geo: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase15_geo_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "geocode_and_aggregate_addresses",
        description = "Locate the addresses in a CSV or spreadsheet in /data offline and return \
            ONLY counts by town, census tract, or census block group, with small counts \
            suppressed (and hidden cells added so they cannot be found by subtraction). \
            Addresses are matched inside the container against MassGIS address points and Census \
            TIGER address ranges (exact point, interpolated between neighbors or along a street \
            range, street only); with no full street index installed it still counts by town, \
            from a town column, a town written in the address, or a ZIP code (through a bundled \
            ZIP-to-town table; ZIP codes cross town lines, so those counts are approximate). \
            It finds the address, town, and ZIP columns from the headers (or you name them), \
            reads only those, refuses to use a name, date of birth, phone, or ID column in their \
            place, and never returns an address, a coordinate, or any other value from the file. \
            Reports how many addresses were located at each precision and the match rate. \
            Optionally writes a file of row number, match level, town, tract, block group, and \
            latitude/longitude to /data/output (identifiable; only its name is returned). The \
            full statewide street index is optional: install it in /data/geocoder or \
            /cache/geocoder (see docs/adr/0011-offline-geocoder.md); a four-town sample is \
            bundled.",
        annotations(
            title = "Geocode and aggregate addresses",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<GeocodeResult>>()
    )]
    async fn geocode_and_aggregate_addresses(
        &self,
        Parameters(args): Parameters<GeocodeArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epi::geocode_and_aggregate_addresses(ctx, args)
        })
        .await
    }
}
