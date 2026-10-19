//! Apache Cassandra dialect (tier A, see SPEC.md §5), on the `scylla` CQL driver (rustls).
//! Works with ScyllaDB and other CQL v4 servers too.
//!
//! The console takes CQL. `USE keyspace` switches the session keyspace. Rows are fetched in pages
//! (`SELECT`), so the row limit stops the download early. The explorer shows keyspaces as
//! schemas, with tables, views, columns, the primary key and secondary indexes.

mod introspect;

use std::{net::ToSocketAddrs, sync::Arc};

use async_trait::async_trait;
use futures_util::StreamExt;
use rowcraft_config::{DataSourceConfig, SslMode};
use rowcraft_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result,
    ResultSet, Value, split_statements,
};
use rowcraft_model::Catalog;
use scylla::{
    client::{session::Session, session_builder::SessionBuilder},
    value::{CqlValue, Row},
};

#[derive(Debug, Default, Clone, Copy)]
pub struct CassandraDialect;

#[async_trait]
impl Dialect for CassandraDialect {
    fn id(&self) -> DialectId {
        DialectId("cassandra")
    }

    fn display_name(&self) -> &'static str {
        "Apache Cassandra"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::WideColumn
    }

    fn default_port(&self) -> Option<u16> {
        Some(9042)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn connects(&self) -> bool {
        true
    }

    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        let host = cfg.host.as_deref().filter(|h| !h.is_empty()).unwrap_or("localhost");
        // Several contact points may be given comma separated; each defaults to the data source port.
        let port = cfg.port.unwrap_or(9042);
        let nodes: Vec<String> = host
            .split(',')
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .map(|h| {
                if h.contains(':') && !h.starts_with('[') && h.matches(':').count() == 1 {
                    h.to_owned()
                } else {
                    format!("{h}:{port}")
                }
            })
            .collect();
        // Names are resolved here so a typo is reported as a plain error, not a driver one.
        for n in &nodes {
            n.to_socket_addrs().map_err(|e| Error::Connect(format!("{n}: {e}")))?;
        }
        let mut builder = SessionBuilder::new()
            .known_nodes(&nodes)
            .connection_timeout(std::time::Duration::from_secs(10));
        if let (Some(u), Some(p)) =
            (cfg.user.as_deref().filter(|u| !u.is_empty()), cfg.password.as_deref())
        {
            builder = builder.user(u, p);
        }
        if let Some(ks) = cfg.database.as_deref().filter(|k| !k.is_empty()) {
            builder = builder.use_keyspace(ks, false);
        }
        if let Some(tls) = tls_config(cfg)? {
            builder = builder.tls_context(Some(tls));
        }
        let session = builder.build().await.map_err(|e| Error::Connect(e.to_string()))?;
        Ok(Box::new(CassandraConnection { session, read_only: cfg.read_only }))
    }

    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        scope: &IntrospectScope,
    ) -> Result<Catalog> {
        introspect::introspect(conn, scope).await
    }
}

fn tls_config(cfg: &DataSourceConfig) -> Result<Option<Arc<rustls::ClientConfig>>> {
    match cfg.ssl_mode {
        SslMode::Disable => return Ok(None),
        SslMode::Auto if cfg.is_loopback_host() => return Ok(None),
        SslMode::Require | SslMode::VerifyCa => {
            return Err(Error::Connect(
                "SSL modes `require` and `verify-ca` are not supported yet; use `verify-full` or `disable`".into(),
            ));
        }
        SslMode::Auto | SslMode::VerifyFull => {}
    }
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Some(ca) = cfg.ssl_files.ca_file.as_deref().filter(|c| !c.is_empty()) {
        let pem = std::fs::read(ca).map_err(|e| Error::Connect(format!("CA file {ca}: {e}")))?;
        for cert in rustls_pemfile_certs(&pem) {
            roots.add(cert).map_err(|e| Error::Connect(format!("CA file {ca}: {e}")))?;
        }
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Connect(e.to_string()))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Some(Arc::new(config)))
}

/// DER certificates of every `CERTIFICATE` block in a PEM file.
fn rustls_pemfile_certs(pem: &[u8]) -> Vec<rustls::pki_types::CertificateDer<'static>> {
    use rustls::pki_types::{CertificateDer, pem::PemObject};
    CertificateDer::pem_slice_iter(pem).filter_map(std::result::Result::ok).collect()
}

#[derive(Debug)]
struct CassandraConnection {
    session: Session,
    read_only: bool,
}

#[async_trait]
impl Connection for CassandraConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let mut last = ResultSet::default();
        for stmt in split_statements(statement) {
            let word = first_word(stmt);
            if self.read_only && !matches!(word.as_str(), "SELECT" | "USE" | "DESCRIBE" | "DESC") {
                return Err(Error::Query(
                    "only SELECT is allowed on a read-only data source".into(),
                ));
            }
            last = match word.as_str() {
                "USE" => {
                    let ks = stmt.trim()["use".len()..].trim().trim_end_matches(';').trim();
                    let (name, case_sensitive) =
                        match ks.strip_prefix('"').and_then(|k| k.strip_suffix('"')) {
                            Some(quoted) => (quoted, true),
                            None => (ks, false),
                        };
                    self.session
                        .use_keyspace(name, case_sensitive)
                        .await
                        .map_err(|e| Error::Query(e.to_string()))?;
                    ResultSet::default()
                }
                "SELECT" => self.select(stmt, max_rows).await?,
                _ => {
                    // DDL and writes: one unpaged request; a few answer with rows (`INSERT … IF NOT EXISTS`).
                    let result = self
                        .session
                        .query_unpaged(stmt, &[])
                        .await
                        .map_err(|e| Error::Query(e.to_string()))?;
                    match result.into_rows_result() {
                        Ok(rows) => {
                            let columns =
                                rows.column_specs().iter().map(|c| c.name().to_owned()).collect();
                            let rows = rows
                                .rows::<Row>()
                                .map_err(|e| Error::Query(e.to_string()))?
                                .filter_map(std::result::Result::ok)
                                .map(|r| r.columns.iter().map(cell).collect())
                                .collect();
                            ResultSet { columns, rows, ..ResultSet::default() }
                        }
                        Err(_) => ResultSet::default(),
                    }
                }
            };
        }
        Ok(last)
    }
}

impl CassandraConnection {
    async fn select(&self, stmt: &str, max_rows: usize) -> Result<ResultSet> {
        let pager =
            self.session.query_iter(stmt, &[]).await.map_err(|e| Error::Query(e.to_string()))?;
        let columns = pager.column_specs().iter().map(|c| c.name().to_owned()).collect();
        let mut stream = pager.rows_stream::<Row>().map_err(|e| Error::Query(e.to_string()))?;
        let mut rs = ResultSet { columns, ..ResultSet::default() };
        while let Some(row) = stream.next().await {
            if rs.rows.len() >= max_rows {
                // Dropping the stream stops fetching further pages.
                rs.truncated = true;
                break;
            }
            let row = row.map_err(|e| Error::Query(e.to_string()))?;
            rs.rows.push(row.columns.iter().map(cell).collect());
        }
        Ok(rs)
    }
}

fn first_word(stmt: &str) -> String {
    stmt.trim_start()
        .split(|c: char| !c.is_ascii_alphabetic())
        .next()
        .unwrap_or("")
        .to_ascii_uppercase()
}

fn cell(v: &Option<CqlValue>) -> Value {
    match v {
        None => Value::Null,
        Some(v) => value(v),
    }
}

fn value(v: &CqlValue) -> Value {
    use CqlValue as C;
    match v {
        C::Ascii(s) | C::Text(s) => Value::Text(s.clone()),
        C::Boolean(b) => Value::Bool(*b),
        C::Blob(b) => Value::Bytes(b.clone()),
        C::TinyInt(n) => Value::Int(i64::from(*n)),
        C::SmallInt(n) => Value::Int(i64::from(*n)),
        C::Int(n) => Value::Int(i64::from(*n)),
        C::BigInt(n) => Value::Int(*n),
        C::Counter(c) => Value::Int(c.0),
        C::Float(f) => Value::Float(f64::from(*f)),
        C::Double(d) => Value::Float(*d),
        C::Empty => Value::Null,
        other => Value::Text(show(other)),
    }
}

/// Text form of a value; collections use CQL literal syntax (`[1, 2]`, `{'a': 1}`).
fn show(v: &CqlValue) -> String {
    use CqlValue as C;
    let list = |items: &[CqlValue]| items.iter().map(show).collect::<Vec<_>>().join(", ");
    match v {
        C::Ascii(s) | C::Text(s) => format!("'{}'", s.replace('\'', "''")),
        C::Boolean(b) => b.to_string(),
        C::TinyInt(n) => n.to_string(),
        C::SmallInt(n) => n.to_string(),
        C::Int(n) => n.to_string(),
        C::BigInt(n) => n.to_string(),
        C::Counter(c) => c.0.to_string(),
        C::Float(f) => f.to_string(),
        C::Double(d) => d.to_string(),
        C::Blob(b) => format!("0x{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>()),
        C::Inet(ip) => ip.to_string(),
        C::Uuid(u) => u.to_string(),
        C::Timeuuid(u) => u.to_string(),
        C::Timestamp(t) => timestamp(t.0),
        C::Date(d) => date(i64::from(d.0) - (1 << 31)),
        C::Time(t) => {
            let n = t.0;
            format!(
                "{:02}:{:02}:{:02}.{:09}",
                n / 3_600_000_000_000,
                n / 60_000_000_000 % 60,
                n / 1_000_000_000 % 60,
                n % 1_000_000_000
            )
        }
        C::Duration(d) => format!("{}mo{}d{}ns", d.months, d.days, d.nanoseconds),
        C::List(items) | C::Vector(items) => format!("[{}]", list(items)),
        C::Set(items) => format!("{{{}}}", list(items)),
        C::Map(pairs) => format!(
            "{{{}}}",
            pairs
                .iter()
                .map(|(k, v)| format!("{}: {}", show(k), show(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        C::Tuple(items) => format!(
            "({})",
            items
                .iter()
                .map(|i| i.as_ref().map_or_else(|| "null".into(), show))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        C::UserDefinedType { fields, .. } => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(n, v)| format!("{n}: {}", v.as_ref().map_or_else(|| "null".into(), show)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        C::Empty => "null".into(),
        // Decimal and varint have no public text form in the driver; Debug is exact if ugly.
        other => format!("{other:?}"),
    }
}

/// `2026-01-31` from days since 1970-01-01 (Howard Hinnant's civil-from-days).
fn date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// `2026-01-31 10:20:30.123` (UTC) from milliseconds since the epoch.
fn timestamp(ms: i64) -> String {
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    format!(
        "{} {:02}:{:02}:{:02}.{:03}",
        date(days),
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(extra: &str) -> DataSourceConfig {
        toml::from_str(&format!("name='c'\ndialect='cassandra'\n{extra}")).unwrap()
    }

    #[test]
    fn temporal_values_format_as_utc() {
        assert_eq!(timestamp(0), "1970-01-01 00:00:00.000");
        assert_eq!(timestamp(1_769_854_830_123), "2026-01-31 10:20:30.123");
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(-1), "1969-12-31");
        assert_eq!(show(&CqlValue::Date(scylla::value::CqlDate((1 << 31) + 20_484))), "2026-01-31");
    }

    #[test]
    fn collections_use_cql_literals() {
        use CqlValue as C;
        assert_eq!(show(&C::List(vec![C::Int(1), C::Int(2)])), "[1, 2]");
        assert_eq!(show(&C::Set(vec![C::Text("a'b".into())])), "{'a''b'}");
        assert_eq!(show(&C::Map(vec![(C::Text("k".into()), C::Boolean(true))])), "{'k': true}");
        assert_eq!(show(&C::Blob(vec![0xde, 0xad])), "0xdead");
        assert_eq!(value(&C::List(vec![C::Int(1)])), Value::Text("[1]".into()));
        assert_eq!(value(&C::BigInt(7)), Value::Int(7));
        assert_eq!(cell(&None), Value::Null);
    }

    #[test]
    fn statement_keywords() {
        assert_eq!(first_word("  select * from t"), "SELECT");
        assert_eq!(first_word("USE ks;"), "USE");
        assert_eq!(first_word("insert into t(a) values (1)"), "INSERT");
    }

    #[test]
    fn tls_follows_ssl_mode() {
        assert!(tls_config(&cfg("")).unwrap().is_none());
        assert!(tls_config(&cfg("host='db.example.com'")).unwrap().is_some());
        assert!(tls_config(&cfg("host='db.example.com'\nssl_mode='disable'")).unwrap().is_none());
        assert!(tls_config(&cfg("ssl_mode='require'")).is_err());
    }
}
