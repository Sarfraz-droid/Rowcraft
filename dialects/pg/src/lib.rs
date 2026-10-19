//! PostgreSQL dialect (tier A, see SPEC.md §5), on `tokio-postgres` with rustls.
//!
//! M0 runs statements through the simple query protocol: multi-statement scripts work and every
//! value arrives as text. TODO(M1): typed decoding via prepared statements, cancel handle
//! (cancel key), transactions.

mod introspect;

use async_trait::async_trait;
use futures_util::{StreamExt, pin_mut};
use rowcraft_config::{DataSourceConfig, SslMode};
use rowcraft_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result,
    ResultSet, Value,
};
use rowcraft_model::Catalog;
use tokio_postgres::{Client, Config, NoTls, SimpleQueryMessage};
use tokio_postgres_rustls::MakeRustlsConnect;

#[derive(Debug, Default, Clone, Copy)]
pub struct PgDialect;

#[async_trait]
impl Dialect for PgDialect {
    fn id(&self) -> DialectId {
        DialectId("pg")
    }

    fn display_name(&self) -> &'static str {
        "PostgreSQL"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        Some(5432)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { transactions: true, ..Capabilities::default() }
    }

    fn connects(&self) -> bool {
        true
    }

    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        connect_wire(cfg, self.default_port()).await
    }

    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        scope: &IntrospectScope,
    ) -> Result<Catalog> {
        introspect::introspect(conn, scope).await
    }
}

/// Opens a PostgreSQL-protocol connection. Shared by every dialect that speaks the pg wire
/// protocol (Redshift, Greenplum, CockroachDB, ...); `default_port` is the dialect's own.
pub async fn connect_wire(
    cfg: &DataSourceConfig,
    default_port: Option<u16>,
) -> Result<Box<dyn Connection>> {
    let mut pg = Config::new();
    pg.host(cfg.host.as_deref().unwrap_or("localhost"));
    pg.port(cfg.port.or(default_port).unwrap_or(5432));
    if let Some(user) = &cfg.user {
        pg.user(user);
    }
    if let Some(password) = &cfg.password {
        pg.password(password);
    }
    if let Some(db) = &cfg.database {
        pg.dbname(db);
    }
    if cfg.read_only {
        pg.options("-c default_transaction_read_only=on");
    }

    let tls = match cfg.ssl_mode {
        SslMode::Disable => false,
        SslMode::VerifyFull => true,
        SslMode::Auto => !cfg.is_loopback_host(),
        // rustls always verifies the server; a mode that skips verification is refused
        // rather than silently run with a stricter check than the user asked for.
        SslMode::Require | SslMode::VerifyCa => {
            return Err(Error::Connect(
                "SSL modes `require` and `verify-ca` are not supported yet; use `verify-full` or `disable`"
                    .into(),
            ));
        }
    };

    let client = if tls {
        pg.ssl_mode(tokio_postgres::config::SslMode::Require);
        // Installing twice is harmless: the second call returns Err and we keep the first.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (client, conn) =
            pg.connect(MakeRustlsConnect::with_webpki_roots()).await.map_err(connect_err)?;
        tokio::spawn(conn);
        client
    } else {
        pg.ssl_mode(tokio_postgres::config::SslMode::Disable);
        let (client, conn) = pg.connect(NoTls).await.map_err(connect_err)?;
        tokio::spawn(conn);
        client
    };
    Ok(Box::new(PgConnection { client }))
}

#[derive(Debug)]
struct PgConnection {
    client: Client,
}

#[async_trait]
impl Connection for PgConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let stream = self.client.simple_query_raw(statement).await.map_err(query_err)?;
        pin_mut!(stream);
        let mut current = ResultSet::default();
        let mut last = ResultSet::default();
        // Rows past the limit are still read (to reach the end of the response) but not kept.
        while let Some(message) = stream.next().await {
            match message.map_err(query_err)? {
                SimpleQueryMessage::RowDescription(columns) => {
                    current = ResultSet {
                        columns: columns.iter().map(|c| c.name().to_owned()).collect(),
                        ..ResultSet::default()
                    };
                }
                SimpleQueryMessage::Row(row) => {
                    if current.columns.is_empty() {
                        current.columns =
                            row.columns().iter().map(|c| c.name().to_owned()).collect();
                    }
                    if current.rows.len() >= max_rows {
                        current.truncated = true;
                        continue;
                    }
                    let cells = (0..row.len())
                        .map(|i| row.get(i).map_or(Value::Null, |s| Value::Text(s.to_owned())))
                        .collect();
                    current.rows.push(cells);
                }
                SimpleQueryMessage::CommandComplete(n) => {
                    current.rows_affected = Some(n);
                    last = std::mem::take(&mut current);
                }
                _ => {}
            }
        }
        Ok(last)
    }
}

fn connect_err(e: tokio_postgres::Error) -> Error {
    Error::Connect(describe(&e))
}

fn query_err(e: tokio_postgres::Error) -> Error {
    Error::Query(describe(&e))
}

/// `db error: ERROR: ...` is noisy; show the server's own message when there is one.
fn describe(e: &tokio_postgres::Error) -> String {
    match e.as_db_error() {
        Some(db) => format!("{}: {}", db.severity(), db.message()),
        None => e.to_string(),
    }
}
