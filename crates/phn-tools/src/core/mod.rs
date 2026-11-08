//! Platform tools (toolset `core`).

pub mod configure;
pub mod export;
pub mod files;
pub mod policies;
pub mod status;

pub use configure::{ConfigureJurisdictionArgs, ConfigureResult, configure_jurisdiction};
pub use export::{ExportDocumentArgs, ExportResult, export_document};
pub use files::{LocalFileResult, ReadLocalFileArgs, read_local_file};
pub use policies::{PolicySearchResult, SearchPoliciesArgs, search_agency_policies};
pub use status::{DataSourceStatus, DataSourceStatusArgs, get_data_source_status};

// Phase 5 modules (CORE-07) go below this line.
pub mod medication;
pub use medication::{MedicationArgs, MedicationResult, lookup_medication_information};

// Phase 9 modules go below this line.
pub mod documentation;
pub mod omaha;
pub mod practice;
pub mod wheel;

pub use documentation::{DocTemplateArgs, DocTemplateResult, nursing_documentation_template};
pub use omaha::{OmahaArgs, OmahaResult, omaha_system_coder};
pub use wheel::{WheelArgs, WheelResult, classify_intervention_wheel};
// End of Phase 9 modules.
