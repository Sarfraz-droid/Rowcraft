//! Exasol dialect (tier B, see SPEC.md §5).
//!
//! TODO: driver/wire integration, introspection queries, type mapping, DDL generator.

use async_trait::async_trait;
use dg_config::DataSourceConfig;
use dg_dialect::{Connection, Dialect, DialectFamily, DialectId, Error, Result};

#[derive(Debug, Default, Clone, Copy)]
pub struct ExasolDialect;

#[async_trait]
impl Dialect for ExasolDialect {
    fn id(&self) -> DialectId {
        DialectId("exasol")
    }

    fn display_name(&self) -> &'static str {
        "Exasol"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        Some(8563)
    }

    async fn connect(&self, _cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        Err(Error::NotImplemented("Exasol"))
    }
}
