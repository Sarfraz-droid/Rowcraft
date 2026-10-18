//! Everything the desktop app does that is not drawing: the "Add data source" form model, the
//! driver catalog as the UI sees it, query results as JSON, and the [`Workbench`] service that
//! connects and runs statements.
//!
//! The desktop shell (`apps/dg-desktop`, Tauri) only forwards its commands to these functions, so
//! the rules are unit-tested here without a webview or a frontend build.

pub mod ai;
pub mod ai_query;
pub mod drivers;
pub mod jdbc_file;
mod query_preview;
pub mod result;
pub mod source_form;
pub mod source_properties;
pub mod sqlite_file;
pub mod workbench;

pub use drivers::{DriverInfo, list_drivers};
pub use result::{QueryOutput, ROW_LIMIT, cell_json, describe_result};
pub use source_form::{Draft, Validated};
pub use workbench::{
    AddedSource, Bootstrap, DriverSettings, FormDefaults, Paths, SourceSummary, Workbench,
};
