//! Google BigQuery dialect (tier B, see SPEC.md §5).
//!
//! TODO: driver/wire integration, introspection queries, type mapping, DDL generator.

use async_trait::async_trait;
use rowcraft_config::DataSourceConfig;
use rowcraft_dialect::{Connection, Dialect, DialectFamily, DialectId, Error, Result};

#[derive(Debug, Default, Clone, Copy)]
pub struct BigqueryDialect;

#[async_trait]
impl Dialect for BigqueryDialect {
    fn id(&self) -> DialectId {
        DialectId("bigquery")
    }

    fn display_name(&self) -> &'static str {
        "Google BigQuery"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        None
    }

    async fn connect(&self, _cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        Err(Error::NotImplemented("Google BigQuery"))
    }
}
