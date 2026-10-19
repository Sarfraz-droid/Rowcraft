//! MariaDB dialect (tier A, see SPEC.md §5): `rowcraft-dialect-mysqlbase` on `mysql_async`.

use async_trait::async_trait;
use rowcraft_config::DataSourceConfig;
use rowcraft_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, IntrospectScope, Result,
};
use rowcraft_dialect_mysqlbase::Flavor;
use rowcraft_model::Catalog;

#[derive(Debug, Default, Clone, Copy)]
pub struct MariadbDialect;

#[async_trait]
impl Dialect for MariadbDialect {
    fn id(&self) -> DialectId {
        DialectId("mariadb")
    }

    fn display_name(&self) -> &'static str {
        "MariaDB"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        Some(3306)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { transactions: true, ..Capabilities::default() }
    }

    fn connects(&self) -> bool {
        true
    }

    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        rowcraft_dialect_mysqlbase::connect(cfg, 3306).await
    }

    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        scope: &IntrospectScope,
    ) -> Result<Catalog> {
        rowcraft_dialect_mysqlbase::introspect(conn, scope, Flavor::MariaDb).await
    }
}
