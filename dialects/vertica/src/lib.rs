//! Vertica dialect (tier C in SPEC.md §5, but the pg v3 wire path covers it): connects with the
//! PostgreSQL protocol client, so the account must use `trust`, `password` or MD5 authentication.
//! Vertica's default SHA-512 password hashing (authentication type 66) is not supported by that
//! client and fails with an authentication error. The catalog is read through `information_schema`.

use async_trait::async_trait;
use dg_config::DataSourceConfig;
use dg_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, IntrospectScope, Result,
};
use dg_model::Catalog;

#[derive(Debug, Default, Clone, Copy)]
pub struct VerticaDialect;

#[async_trait]
impl Dialect for VerticaDialect {
    fn id(&self) -> DialectId {
        DialectId("vertica")
    }

    fn display_name(&self) -> &'static str {
        "Vertica"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        Some(5433)
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
