//! SQL Server dialect (tier A, see SPEC.md §5), on `tiberius` (TDS 7.x, rustls).
//!
//! Also serves Azure SQL Database and Azure Synapse, which the driver catalog maps here.
//! SQL authentication only for now; integrated (NTLM/Kerberos) auth is `tiberius`'s `sspi-rs`
//! feature and is not enabled yet. Named instances (`host\instance`) need SQL Browser and are
//! refused with a clear message: give the instance's port instead.

mod introspect;

use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use dg_config::{DataSourceConfig, SslMode};
use dg_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result,
    ResultSet, Value,
};
use dg_model::Catalog;
use futures_util::StreamExt;
use tiberius::{AuthMethod, Client, ColumnData, Config, EncryptionLevel, FromSql, QueryItem};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

#[derive(Debug, Default, Clone, Copy)]
pub struct MssqlDialect;

#[async_trait]
impl Dialect for MssqlDialect {
    fn id(&self) -> DialectId {
        DialectId("mssql")
    }

    fn display_name(&self) -> &'static str {
        "Microsoft SQL Server"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        Some(1433)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { transactions: true, ..Capabilities::default() }
    }

    fn connects(&self) -> bool {
        true
    }

    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        let config = build_config(cfg)?;
        let tcp = TcpStream::connect(config.get_addr())
            .await
            .map_err(|e| Error::Connect(format!("{}: {e}", config.get_addr())))?;
        tcp.set_nodelay(true).map_err(|e| Error::Connect(e.to_string()))?;
        let client = Client::connect(config, tcp.compat_write()).await.map_err(connect_err)?;
        Ok(Box::new(MssqlConnection { client }))
    }

    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        scope: &IntrospectScope,
    ) -> Result<Catalog> {
        introspect::introspect(conn, scope).await
    }
}

fn build_config(cfg: &DataSourceConfig) -> Result<Config> {
    let host = cfg.host.as_deref().unwrap_or("localhost");
    if host.contains('\\') {
        return Err(Error::Connect(
            "named instances (`host\\instance`) are not supported; use the instance's port".into(),
        ));
    }
    let mut c = Config::new();
    c.host(host);
    c.port(cfg.port.unwrap_or(1433));
    if let Some(db) = cfg.database.as_deref().filter(|d| !d.is_empty()) {
        c.database(db);
    }
    c.authentication(AuthMethod::sql_server(
        cfg.user.as_deref().unwrap_or("sa"),
        cfg.password.as_deref().unwrap_or(""),
    ));
    c.application_name("Rowcraft");
    c.readonly(cfg.read_only);
    match cfg.ssl_mode {
        SslMode::Disable => c.encryption(EncryptionLevel::NotSupported),
        // Login is always encrypted by the server when it insists; only the data stream is
        // plain on a loopback connection.
        SslMode::Auto if cfg.is_loopback_host() => c.encryption(EncryptionLevel::Off),
        SslMode::Auto | SslMode::VerifyFull | SslMode::VerifyCa => {
            c.encryption(EncryptionLevel::Required);
        }
        SslMode::Require => {
            c.encryption(EncryptionLevel::Required);
            c.trust_cert();
        }
    }
    if let Some(ca) = cfg.ssl_files.ca_file.as_deref().filter(|f| !f.is_empty()) {
        c.trust_cert_ca(ca);
    }
    Ok(c)
}

#[derive(Debug)]
struct MssqlConnection {
    client: Client<Compat<TcpStream>>,
}

#[async_trait]
impl Connection for MssqlConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let mut stream = self.client.simple_query(statement).await.map_err(query_err)?;
        let mut last = ResultSet::default();
        let mut current: Option<ResultSet> = None;
        while let Some(item) = stream.next().await {
            match item.map_err(query_err)? {
                QueryItem::Metadata(meta) => {
                    if let Some(done) = current.take() {
                        last = done;
                    }
                    current = Some(ResultSet {
                        columns: meta.columns().iter().map(|c| c.name().to_owned()).collect(),
                        ..ResultSet::default()
                    });
                }
                QueryItem::Row(row) => {
                    let set = current.get_or_insert_with(ResultSet::default);
                    if set.rows.len() >= max_rows {
                        set.truncated = true;
                        continue;
                    }
                    set.rows.push(row.cells().map(|(_, d)| cell(d)).collect());
                }
            }
        }
        Ok(current.unwrap_or(last))
    }
}

fn cell(d: &ColumnData<'static>) -> Value {
    use ColumnData as D;
    match d {
        D::U8(v) => v.map_or(Value::Null, |v| Value::Int(i64::from(v))),
        D::I16(v) => v.map_or(Value::Null, |v| Value::Int(i64::from(v))),
        D::I32(v) => v.map_or(Value::Null, |v| Value::Int(i64::from(v))),
        D::I64(v) => v.map_or(Value::Null, Value::Int),
        D::F32(v) => v.map_or(Value::Null, |v| Value::Float(f64::from(v))),
        D::F64(v) => v.map_or(Value::Null, Value::Float),
        D::Bit(v) => v.map_or(Value::Null, Value::Bool),
        D::String(v) => v.as_ref().map_or(Value::Null, |s| Value::Text(s.to_string())),
        D::Guid(v) => v.map_or(Value::Null, |g| Value::Text(g.to_string())),
        D::Binary(v) => v.as_ref().map_or(Value::Null, |b| Value::Bytes(b.to_vec())),
        D::Numeric(v) => v.map_or(Value::Null, |n| Value::Text(n.to_string())),
        D::Xml(v) => v.as_ref().map_or(Value::Null, |x| Value::Text(x.to_string())),
        D::DateTime(_) | D::SmallDateTime(_) | D::DateTime2(_) => {
            temporal::<NaiveDateTime>(d, |t| t.format("%Y-%m-%d %H:%M:%S%.f").to_string())
        }
        D::Date(_) => temporal::<NaiveDate>(d, |t| t.format("%Y-%m-%d").to_string()),
        D::Time(_) => temporal::<NaiveTime>(d, |t| t.format("%H:%M:%S%.f").to_string()),
        D::DateTimeOffset(_) => temporal::<chrono::DateTime<chrono::Utc>>(d, |t| {
            t.format("%Y-%m-%d %H:%M:%S%.f +00:00").to_string()
        }),
    }
}

fn temporal<T: for<'a> FromSql<'a>>(d: &ColumnData<'static>, fmt: impl Fn(&T) -> String) -> Value {
    match T::from_sql(d) {
        Ok(Some(t)) => Value::Text(fmt(&t)),
        _ => Value::Null,
    }
}

fn connect_err(e: tiberius::error::Error) -> Error {
    Error::Connect(e.to_string())
}

fn query_err(e: tiberius::error::Error) -> Error {
    Error::Query(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(extra: &str) -> DataSourceConfig {
        toml::from_str(&format!("name='m'\ndialect='mssql'\n{extra}")).unwrap()
    }

    #[test]
    fn named_instances_are_refused() {
        let e = build_config(&cfg("host='srv\\\\sqlexpress'")).unwrap_err();
        assert!(e.to_string().contains("named instances"));
    }

    #[test]
    fn address_uses_default_port() {
        assert_eq!(
            build_config(&cfg("host='db.example.com'")).unwrap().get_addr(),
            "db.example.com:1433"
        );
        assert_eq!(build_config(&cfg("host='h'\nport=14330")).unwrap().get_addr(), "h:14330");
    }

    #[test]
    fn values_map_to_cells() {
        assert_eq!(cell(&ColumnData::I32(Some(7))), Value::Int(7));
        assert_eq!(cell(&ColumnData::I32(None)), Value::Null);
        assert_eq!(cell(&ColumnData::String(Some("x".into()))), Value::Text("x".into()));
        assert_eq!(cell(&ColumnData::Bit(Some(true))), Value::Bool(true));
        assert_eq!(cell(&ColumnData::Binary(Some(vec![1, 2].into()))), Value::Bytes(vec![1, 2]));
    }
}
