//! H2 dialect (tier A, see SPEC.md §5), through H2's PostgreSQL server mode.
//!
//! Start the server with `java -cp h2.jar org.h2.tools.Server -pg` (default port 5435); the H2
//! TCP protocol itself is not implemented. The catalog is read through `information_schema`
//! (`rowcraft-dialect-pgbase`).

use async_trait::async_trait;
use rowcraft_config::DataSourceConfig;
use rowcraft_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, IntrospectScope, Result,
};
use rowcraft_model::Catalog;

#[derive(Debug, Default, Clone, Copy)]
pub struct H2Dialect;

#[async_trait]
impl Dialect for H2Dialect {
    fn id(&self) -> DialectId {
        DialectId("h2")
    }

    fn display_name(&self) -> &'static str {
        "H2"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        Some(5435)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { transactions: true, ..Capabilities::default() }
    }

    fn connects(&self) -> bool {
        true
    }

    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        rowcraft_dialect_pg::connect_wire(cfg, self.default_port()).await
    }

    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        scope: &IntrospectScope,
    ) -> Result<Catalog> {
        rowcraft_dialect_pgbase::introspect_information_schema(conn, scope).await
    }
}
