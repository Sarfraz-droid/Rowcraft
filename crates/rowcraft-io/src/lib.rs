//! Extractors (export) and loaders (import).
//!
//! M0 ships the result writers used by `rowcraft query`. TODO(M2): streaming export of full queries,
//! SQL INSERT / XML / HTML / XLSX writers, import with column mapping.

mod format;

pub use format::{Format, ParseFormatError, write_result};
