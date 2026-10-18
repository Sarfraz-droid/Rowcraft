//! Shared MySQL-protocol code for MySQL and MariaDB (tier A, see SPEC.md §5), on `mysql_async`
//! with rustls. Also serves the databases DataGrip lists that speak the same protocol (Aurora
//! MySQL, TiDB, SingleStore): the driver catalog maps them to the `mysql` dialect.
//!
//! Statements run through the text protocol, so values arrive as text like in the pg dialect.

mod introspect;

use async_trait::async_trait;
use dg_config::{DataSourceConfig, SslMode};
use dg_dialect::{Connection, Error, IntrospectScope, Result, ResultSet, Value};
use dg_model::Catalog;
use futures_util::TryStreamExt;
use mysql_async::{Conn, OptsBuilder, Row, SslOpts, prelude::Queryable};

/// The two servers differ in a handful of `information_schema` columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    MySql,
    MariaDb,
}

/// Opens a connection. `default_port` is the dialect's own (3306 for both today).
pub async fn connect(cfg: &DataSourceConfig, default_port: u16) -> Result<Box<dyn Connection>> {
    let host = cfg.host.as_deref().unwrap_or("localhost");
    let mut opts = OptsBuilder::default()
        .ip_or_hostname(host)
        .tcp_port(cfg.port.unwrap_or(default_port))
        .prefer_socket(false)
        .user(cfg.user.as_deref())
        .pass(cfg.password.as_deref())
        .db_name(cfg.database.as_deref().filter(|d| !d.is_empty()));
    if let Some(ssl) = ssl_opts(cfg)? {
        // Installing twice is harmless: the second call returns Err and we keep the first.
        let _ = rustls::crypto::ring::default_provider().install_default();
        opts = opts.ssl_opts(ssl);
    }
    let mut conn = Conn::new(opts).await.map_err(|e| Error::Connect(describe(&e)))?;
    if cfg.read_only {
        conn.query_drop("SET SESSION TRANSACTION READ ONLY").await.map_err(query_err)?;
    }
    Ok(Box::new(MysqlConnection { conn }))
}

fn ssl_opts(cfg: &DataSourceConfig) -> Result<Option<SslOpts>> {
    let mut ssl = SslOpts::default();
    match cfg.ssl_mode {
        SslMode::Disable => return Ok(None),
        SslMode::Auto if cfg.is_loopback_host() => return Ok(None),
        SslMode::Auto | SslMode::VerifyFull => {}
        // Encrypt, but do not check the certificate chain / the host name.
        SslMode::Require => ssl = ssl.with_danger_accept_invalid_certs(true),
        SslMode::VerifyCa => ssl = ssl.with_danger_skip_domain_validation(true),
    }
    if let Some(ca) = cfg.ssl_files.ca_file.as_deref().filter(|c| !c.is_empty()) {
        ssl = ssl.with_root_certs(vec![std::path::PathBuf::from(ca).into()]);
    }
    Ok(Some(ssl))
}

pub async fn introspect(
    conn: &mut dyn Connection,
    scope: &IntrospectScope,
    flavor: Flavor,
) -> Result<Catalog> {
    introspect::introspect(conn, scope, flavor).await
}

#[derive(Debug)]
struct MysqlConnection {
    conn: Conn,
}

#[async_trait]
impl Connection for MysqlConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let mut result = self.conn.query_iter(statement).await.map_err(query_err)?;
        let mut last = ResultSet::default();
        // `stream` yields one result set at a time; a script returns the last one.
        while let Some(mut set) = result.stream::<Row>().await.map_err(query_err)? {
            let columns = set.columns();
            let mut current = ResultSet {
                columns: columns.iter().map(|c| c.name_str().into_owned()).collect(),
                ..ResultSet::default()
            };
            let binary: Vec<bool> = columns.iter().map(|c| is_binary(c)).collect();
            while let Some(row) = set.try_next().await.map_err(query_err)? {
                // Rows past the limit are read (to reach the end of the response) but dropped.
                if current.rows.len() >= max_rows {
                    current.truncated = true;
                    continue;
                }
                current.rows.push(
                    (0..row.len())
                        .map(|i| cell(row.as_ref(i), binary.get(i).copied().unwrap_or(false)))
                        .collect(),
                );
            }
            if columns.is_empty() {
                current.rows_affected = Some(set.affected_rows());
            }
            last = current;
        }
        Ok(last)
    }
}

/// Binary string columns (BLOB, BINARY, VARBINARY, BIT, geometry). Numbers and dates also carry
/// the `binary` charset (63) in a text result, so the column type has to be checked as well.
fn is_binary(c: &mysql_async::Column) -> bool {
    use mysql_async::consts::ColumnType as T;
    c.character_set() == 63
        && matches!(
            c.column_type(),
            T::MYSQL_TYPE_BLOB
                | T::MYSQL_TYPE_TINY_BLOB
                | T::MYSQL_TYPE_MEDIUM_BLOB
                | T::MYSQL_TYPE_LONG_BLOB
                | T::MYSQL_TYPE_VAR_STRING
                | T::MYSQL_TYPE_STRING
                | T::MYSQL_TYPE_VARCHAR
                | T::MYSQL_TYPE_BIT
                | T::MYSQL_TYPE_GEOMETRY
        )
}

fn cell(v: Option<&mysql_async::Value>, binary: bool) -> Value {
    use mysql_async::Value as V;
    match v {
        None | Some(V::NULL) => Value::Null,
        Some(V::Bytes(b)) if binary => Value::Bytes(b.clone()),
        Some(V::Bytes(b)) => Value::Text(String::from_utf8_lossy(b).into_owned()),
        Some(V::Int(i)) => Value::Int(*i),
        Some(V::UInt(u)) => {
            i64::try_from(*u).map_or_else(|_| Value::Text(u.to_string()), Value::Int)
        }
        Some(V::Float(f)) => Value::Float(f64::from(*f)),
        Some(V::Double(d)) => Value::Float(*d),
        // Only the binary protocol produces these; format them like the text protocol would.
        Some(V::Date(y, mo, d, h, mi, s, us)) => Value::Text(format!(
            "{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}{}",
            if *us > 0 { format!(".{us:06}") } else { String::new() }
        )),
        Some(V::Time(neg, days, h, mi, s, us)) => Value::Text(format!(
            "{}{:02}:{mi:02}:{s:02}{}",
            if *neg { "-" } else { "" },
            u32::from(*h) + days * 24,
            if *us > 0 { format!(".{us:06}") } else { String::new() }
        )),
    }
}

fn describe(e: &mysql_async::Error) -> String {
    match e {
        mysql_async::Error::Server(s) => format!("ERROR {} ({}): {}", s.code, s.state, s.message),
        other => other.to_string(),
    }
}

fn query_err(e: mysql_async::Error) -> Error {
    Error::Query(describe(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_auto_is_plain_and_remote_auto_is_tls() {
        let mut cfg = DataSourceConfig { host: Some("localhost".into()), ..sample() };
        assert!(ssl_opts(&cfg).unwrap().is_none());
        cfg.host = Some("db.example.com".into());
        assert!(ssl_opts(&cfg).unwrap().is_some());
        cfg.ssl_mode = SslMode::Disable;
        assert!(ssl_opts(&cfg).unwrap().is_none());
    }

    fn sample() -> DataSourceConfig {
        toml::from_str("name = 't'\ndialect = 'mysql'").unwrap()
    }

    #[test]
    fn values_map_to_cells() {
        use mysql_async::Value as V;
        assert_eq!(cell(Some(&V::NULL), false), Value::Null);
        assert_eq!(cell(Some(&V::Bytes(b"hi".to_vec())), false), Value::Text("hi".into()));
        assert_eq!(cell(Some(&V::Bytes(vec![0xff])), true), Value::Bytes(vec![0xff]));
        assert_eq!(cell(Some(&V::UInt(u64::MAX)), false), Value::Text(u64::MAX.to_string()));
        assert_eq!(
            cell(Some(&V::Date(2026, 1, 2, 3, 4, 5, 0)), false),
            Value::Text("2026-01-02 03:04:05".into())
        );
    }
}
