//! HSQLDB dialect (tier C, see SPEC.md §5).
//!
//! TODO: driver/wire integration, introspection queries, type mapping, DDL generator.

use async_trait::async_trait;
use rowcraft_config::DataSourceConfig;
use rowcraft_dialect::{Connection, Dialect, DialectFamily, DialectId, Error, Result};

#[derive(Debug, Default, Clone, Copy)]
pub struct HsqldbDialect;

#[async_trait]
impl Dialect for HsqldbDialect {
    fn id(&self) -> DialectId {
        DialectId("hsqldb")
    }

    fn display_name(&self) -> &'static str {
        "HSQLDB"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        Some(9001)
    }

    async fn connect(&self, _cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        Err(Error::NotImplemented("HSQLDB"))
    }
}
