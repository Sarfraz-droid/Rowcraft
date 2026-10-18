//! Oracle dialect (tier C, see SPEC.md §5).
//!
//! TODO: driver/wire integration, introspection queries, type mapping, DDL generator.

use async_trait::async_trait;
use dg_config::DataSourceConfig;
use dg_dialect::{Connection, Dialect, DialectFamily, DialectId, Error, Result};

#[derive(Debug, Default, Clone, Copy)]
pub struct OracleDialect;

#[async_trait]
impl Dialect for OracleDialect {
    fn id(&self) -> DialectId {
        DialectId("oracle")
    }

    fn display_name(&self) -> &'static str {
        "Oracle"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        Some(1521)
    }

    async fn connect(&self, _cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        Err(Error::NotImplemented("Oracle"))
    }
}
