//! MySQL dialect (tier A, see SPEC.md §5): `dg-dialect-mysqlbase` on `mysql_async`.

use async_trait::async_trait;
use dg_config::DataSourceConfig;
use dg_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, IntrospectScope, Result,
};
use dg_dialect_mysqlbase::Flavor;
use dg_model::Catalog;

#[derive(Debug, Default, Clone, Copy)]
pub struct MysqlDialect;

#[async_trait]
impl Dialect for MysqlDialect {
    fn id(&self) -> DialectId {
        DialectId("mysql")
    }

    fn display_name(&self) -> &'static str {
        "MySQL"
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
        dg_dialect_mysqlbase::connect(cfg, 3306).await
    }

    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        scope: &IntrospectScope,
    ) -> Result<Catalog> {
        dg_dialect_mysqlbase::introspect(conn, scope, Flavor::MySql).await
    }
}
