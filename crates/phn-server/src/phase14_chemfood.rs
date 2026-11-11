//! Phase 14 tools (environmental health and climate). Chemicals and food: ENV-08 chemical_exposure_reference, CHR-06 food_access_mapper.

use phn_core::Envelope;
use phn_tools::chronic::{FoodAccessArgs, FoodAccessResult};
use phn_tools::env::{ChemicalArgs, ChemicalResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

#[allow(unused_imports)]
use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase14_chemfood_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "chemical_exposure_reference",
        description = "Reference for a community chemical exposure question (spill, release, \
            contaminated site, indoor air, drinking water, workplace). Look up a chemical by \
            name, synonym, trade name, or CAS number (offers candidates when the name is \
            ambiguous) and get its NIOSH Pocket Guide entry (REL, PEL, IDLH, physical \
            description, exposure routes, symptoms, target organs, first aid), the current NIOSH \
            IDLH, ATSDR minimal risk levels by route and duration, and links to the ATSDR \
            ToxFAQs fact sheet. Give a scenario for scenario notes and whom to call in \
            Massachusetts (911, Poison Control, MassDEP's 24-hour spill line, the National \
            Response Center, DPH toxicology and indoor air, the local board of health). Every \
            answer says that occupational limits are not community limits and that MRLs are \
            screening levels. Bundled snapshots; works offline.",
        annotations(
            title = "Chemical exposure reference",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<ChemicalResult>>()
    )]
    async fn chemical_exposure_reference(
        &self,
        Parameters(args): Parameters<ChemicalArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::env::chemical_exposure_reference(ctx, args)
        })
        .await
    }

    #[tool(
        name = "food_access_mapper",
        description = "Food access for a Massachusetts town, county, or the state (default: \
            your saved town): low-income/low-access census tracts from the USDA Food Access \
            Research Atlas under each definition (1 mile urban/10 rural, 1/2 mile/10, 1 mile/20, \
            vehicle access) by straight-line or driving distance to a SNAP-authorized store \
            (2025, 2020 tracts), with the 2019 supermarket-based measure for comparison; people \
            and low-income people affected, households without a vehicle or receiving SNAP, \
            poverty, and SVI. Adds HIP (Healthy Incentives Program) and SNAP information, and \
            counts by town of the food pantries, SNAP retailers, and HIP vendors in your own \
            files (site names and addresses go only to the output files). Writes a tract map \
            (SVG/PNG), a DOCX resource list, and an XLSX workbook to /data/output.",
        annotations(
            title = "Food access mapper",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<FoodAccessResult>>()
    )]
    async fn food_access_mapper(
        &self,
        Parameters(args): Parameters<FoodAccessArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::chronic::food_access_mapper(ctx, args)
        })
        .await
    }
}
