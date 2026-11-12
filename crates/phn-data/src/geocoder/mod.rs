//! Offline geocoder for Massachusetts addresses (EPI-13).
//!
//! * [`normalize`]: USPS-style street-name normalization and address parsing.
//! * [`index`]: the index format (MassGIS address points and TIGER address
//!   ranges by town), lookup, and installed or bundled index discovery.
//! * [`crosswalk`]: ZIP code to town and place name to town, for the town-level
//!   fallback when no street index is installed.
//!
//! The index format and build are described in `docs/adr/0011-offline-geocoder.md`
//! and `xtask/scripts/build_geocoder_index.py`.

pub mod crosswalk;
pub mod index;
pub mod normalize;

pub use crosswalk::{Crosswalk, ZipTown};
pub use index::{IndexInfo, IndexSet, Located, MatchLevel, TownIndex};
pub use normalize::{ParsedAddress, normalize_place, normalize_street, parse_address};
