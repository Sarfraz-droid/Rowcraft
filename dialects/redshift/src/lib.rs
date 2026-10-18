//! Amazon Redshift dialect (tier A, see SPEC.md §5): the PostgreSQL wire protocol on `dg-dialect-pg`,
//! catalog read through `information_schema` (`dg-dialect-pgbase`).

use async_trait::async_trait;
use dg_config::DataSourceConfig;
use dg_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, IntrospectScope, Result,
};
use dg_model::Catalog;

#[derive(Debug, Default, Clone, Copy)]
pub struct RedshiftDialect;

#[async_trait]
impl Dialect for RedshiftDialect {
    fn id(&self) -> DialectId {
        DialectId("redshift")
    }

    fn display_name(&self) -> &'static str {
        "Amazon Redshift"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        Some(5439)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { transactions: true, ..Capabilities::default() }
    }

    fn connects(&self) -> bool {
        true
    }

    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        dg_dialect_pg::connect_wire(cfg, self.default_port()).await
    }

    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        scope: &IntrospectScope,
    ) -> Result<Catalog> {
        dg_dialect_pgbase::introspect_information_schema(conn, scope).await
    }
}
