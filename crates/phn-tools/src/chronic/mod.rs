//! Chronic disease prevention tools (toolset `chronic`).

// Phase 4 modules (CHR-01) go below this line.
pub mod places;

pub use places::{PlacesArgs, PlacesResult, get_places_estimates};

// Phase 14 modules (CHR-06, CHR-07) go below this line.
// Phase 14 Water (ENV-03, CHR-07).
pub mod fluoridation;

pub use fluoridation::{FluoridationArgs, FluoridationResult, community_water_fluoridation_status};
// End of Phase 14 water.
// Phase 14 EJ and climate (ENV-04, ENV-05).
// End of Phase 14 ejclimate.
// Phase 14 Vectors and housing (ENV-06, ENV-07).
// End of Phase 14 vectorhousing.
// Phase 14 Chemicals and food (ENV-08, CHR-06).
pub mod food_access;

pub use food_access::{FoodAccessArgs, FoodAccessResult, food_access_mapper};
// End of Phase 14 chemfood.
// End of Phase 14 modules (CHR-06, CHR-07).

// Phase 15 screen (CHR-02, CHR-03): add `pub mod` and `pub use` lines below.
// End of Phase 15 screen.

// Phase 15 prevent (CHR-04, CHR-05): add `pub mod` and `pub use` lines below.
pub mod asthma_home;
pub mod preventive;

pub use asthma_home::{AsthmaHomeArgs, AsthmaHomeResult, asthma_home_assessment};
pub use preventive::{PreventiveArgs, PreventiveResult, preventive_services_lookup};
// End of Phase 15 prevent.
