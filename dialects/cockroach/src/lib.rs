//! CockroachDB dialect (tier A, see SPEC.md §5): the PostgreSQL wire protocol on `dg-dialect-pg`,
//! catalog read through `information_schema` (`dg-dialect-pgbase`).

use async_trait::async_trait;
use dg_config::DataSourceConfig;
use dg_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, IntrospectScope, Result,
};
use dg_model::Catalog;

#[derive(Debug, Default, Clone, Copy)]
pub struct CockroachDialect;

#[async_trait]
impl Dialect for CockroachDialect {
    fn id(&self) -> DialectId {
        DialectId("cockroach")
    }

    fn display_name(&self) -> &'static str {
        "CockroachDB"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        Some(26257)
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
