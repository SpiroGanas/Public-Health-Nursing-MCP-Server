//! Platform tools (toolset `core`).

pub mod configure;
pub mod status;

pub use configure::{ConfigureJurisdictionArgs, ConfigureResult, configure_jurisdiction};
pub use status::{DataSourceStatus, DataSourceStatusArgs, get_data_source_status};
