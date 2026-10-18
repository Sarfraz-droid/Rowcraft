//! Redis dialect (tier A, see SPEC.md §5), on the `redis` crate (RESP2/3, rustls).
//!
//! The "query" is a Redis command line: `GET key`, `HGETALL "my key"`. Replies become a result
//! set: a scalar is one row, an array one row per element (nested arrays are flattened depth
//! first), a map two columns. Several commands may be separated by newlines.
//!
//! The explorer shows the current database as one schema whose tables are key prefixes (the part
//! before the first `:`), each with a single `key` column typed after the Redis type.

use async_trait::async_trait;
use dg_config::{DataSourceConfig, SslMode};
use dg_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result,
    ResultSet, Value,
};
use dg_model::{Catalog, Column, Schema, Table};
use redis::aio::MultiplexedConnection;

/// Keys looked at when building the explorer tree.
const SCAN_LIMIT: usize = 10_000;

#[derive(Debug, Default, Clone, Copy)]
pub struct RedisDialect;

#[async_trait]
impl Dialect for RedisDialect {
    fn id(&self) -> DialectId {
        DialectId("redis")
    }

    fn display_name(&self) -> &'static str {
        "Redis"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::KeyValue
    }

    fn default_port(&self) -> Option<u16> {
        Some(6379)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { transactions: false, ..Capabilities::default() }
    }

    fn connects(&self) -> bool {
        true
    }

    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        let url = connection_url(cfg)?;
        if url.starts_with("rediss://") {
            // Installing twice is harmless: the second call returns Err and we keep the first.
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let client = redis::Client::open(url).map_err(|e| Error::Connect(e.to_string()))?;
        let mut con = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| Error::Connect(e.to_string()))?;
        // A connection is lazy about auth and db selection; PING surfaces both errors now.
        redis::cmd("PING")
            .query_async::<redis::Value>(&mut con)
            .await
            .map_err(|e| Error::Connect(e.to_string()))?;
        if cfg.read_only {
            // No server-side read-only session exists; commands are screened instead.
        }
        Ok(Box::new(RedisConnection { con, read_only: cfg.read_only }))
    }

    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        _scope: &IntrospectScope,
    ) -> Result<Catalog> {
        let db = current_db(conn).await;
        let mut keys: Vec<String> = Vec::new();
        let mut cursor = "0".to_owned();
        loop {
            let reply = conn.query(&format!("SCAN {cursor} COUNT 1000")).await?;
            // Flattened reply: the next cursor first, then the keys.
            let mut rows = reply.rows.into_iter().filter_map(|r| text(r.first()?));
            cursor = rows.next().unwrap_or_else(|| "0".into());
            keys.extend(rows);
            if cursor == "0" || keys.len() >= SCAN_LIMIT {
                break;
            }
        }
        let mut groups: std::collections::BTreeMap<String, String> = Default::default();
        for key in &keys {
            let prefix = key.split_once(':').map_or(key.as_str(), |(p, _)| p).to_owned();
            groups.entry(prefix).or_insert_with(|| key.clone());
        }
        let mut tables = Vec::new();
        for (prefix, sample) in groups {
            let ty = conn
                .query(&format!("TYPE {}", quote(&sample)))
                .await
                .ok()
                .and_then(|r| r.rows.first().and_then(|r| text(r.first()?)))
                .unwrap_or_else(|| "string".into());
            tables.push(Table {
                name: prefix,
                columns: vec![Column { name: "key".into(), native_type: ty, ..Column::default() }],
                ..Table::default()
            });
        }
        let name = format!("db{db}");
        Ok(Catalog {
            database: Some(db.to_string()),
            default_schema: Some(name.clone()),
            schemas: vec![Schema { name, tables, ..Schema::default() }],
        })
    }
}

fn text(v: &Value) -> Option<String> {
    match v {
        Value::Text(s) => Some(s.clone()),
        Value::Int(i) => Some(i.to_string()),
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

async fn current_db(conn: &mut dyn Connection) -> u32 {
    // `CLIENT INFO` (Redis 6.2+) lists `db=N`.
    conn.query("CLIENT INFO")
        .await
        .ok()
        .and_then(|r| r.rows.first().and_then(|r| text(r.first()?)))
        .and_then(|info| {
            info.split_whitespace().find_map(|kv| kv.strip_prefix("db=")?.parse().ok())
        })
        .unwrap_or(0)
}

/// `redis://[user:password@]host:port/db`, or `rediss://` when TLS is wanted.
fn connection_url(cfg: &DataSourceConfig) -> Result<String> {
    let tls = match cfg.ssl_mode {
        SslMode::Disable => false,
        SslMode::VerifyFull => true,
        SslMode::Auto => !cfg.is_loopback_host(),
        SslMode::Require | SslMode::VerifyCa => {
            return Err(Error::Connect(
                "SSL modes `require` and `verify-ca` are not supported yet; use `verify-full` or `disable`"
                    .into(),
            ));
        }
    };
    let db: u32 = match cfg.database.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        None => 0,
        Some(d) => d
            .parse()
            .map_err(|_| Error::Connect(format!("Redis database must be a number, got `{d}`")))?,
    };
    let auth = match (cfg.user.as_deref().filter(|u| !u.is_empty()), cfg.password.as_deref()) {
        (Some(u), Some(p)) => format!("{}:{}@", encode(u), encode(p)),
        (None, Some(p)) => format!(":{}@", encode(p)),
        (Some(u), None) => format!("{}@", encode(u)),
        (None, None) => String::new(),
    };
    let host = cfg.host.as_deref().unwrap_or("localhost");
    let host = if host.contains(':') { format!("[{host}]") } else { host.to_owned() };
    Ok(format!(
        "{}://{auth}{host}:{}/{db}",
        if tls { "rediss" } else { "redis" },
        cfg.port.unwrap_or(6379)
    ))
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Debug)]
struct RedisConnection {
    con: MultiplexedConnection,
    read_only: bool,
}

#[async_trait]
impl Connection for RedisConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let mut last = ResultSet::default();
        for line in
            statement.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let words = split_command(line).map_err(Error::Query)?;
            let Some((name, args)) = words.split_first() else { continue };
            let name = String::from_utf8_lossy(name).into_owned();
            if self.read_only && !is_read_only(&name) {
                return Err(Error::Query(format!(
                    "`{name}` is not allowed on a read-only data source"
                )));
            }
            let mut cmd = redis::cmd(&name);
            for a in args {
                cmd.arg(a.as_slice());
            }
            let reply = cmd
                .query_async::<redis::Value>(&mut self.con)
                .await
                .map_err(|e| Error::Query(e.to_string()))?;
            last = to_result(&reply, max_rows);
        }
        Ok(last)
    }
}

/// Commands that cannot change data, for read-only data sources.
fn is_read_only(name: &str) -> bool {
    const READ: &[&str] = &[
        "GET",
        "MGET",
        "STRLEN",
        "GETRANGE",
        "EXISTS",
        "TYPE",
        "TTL",
        "PTTL",
        "KEYS",
        "SCAN",
        "DBSIZE",
        "HGET",
        "HGETALL",
        "HKEYS",
        "HVALS",
        "HLEN",
        "HMGET",
        "HEXISTS",
        "HSCAN",
        "LRANGE",
        "LLEN",
        "LINDEX",
        "SMEMBERS",
        "SCARD",
        "SISMEMBER",
        "SSCAN",
        "SRANDMEMBER",
        "ZRANGE",
        "ZRANGEBYSCORE",
        "ZCARD",
        "ZSCORE",
        "ZRANK",
        "ZCOUNT",
        "ZSCAN",
        "XRANGE",
        "XLEN",
        "XREAD",
        "INFO",
        "PING",
        "ECHO",
        "TIME",
        "CLIENT",
        "CONFIG",
        "MEMORY",
        "OBJECT",
        "SELECT",
        "COMMAND",
        "LOLWUT",
        "RANDOMKEY",
        "PUBSUB",
        "BITCOUNT",
        "GETBIT",
        "PFCOUNT",
        "GEOPOS",
        "GEODIST",
        "GEOSEARCH",
        "JSON.GET",
    ];
    READ.contains(&name.to_ascii_uppercase().as_str())
}

/// Splits a command line like a shell: whitespace separates words, `"..."` and `'...'` group
/// them, and `\n \r \t \\ \" \xHH` work inside double quotes.
fn split_command(line: &str) -> std::result::Result<Vec<Vec<u8>>, String> {
    let bytes = line.as_bytes();
    let (mut words, mut i) = (Vec::new(), 0);
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let mut word = Vec::new();
        match bytes[i] {
            q @ (b'"' | b'\'') => {
                i += 1;
                loop {
                    let Some(&c) = bytes.get(i) else {
                        return Err("unterminated quoted string".into());
                    };
                    i += 1;
                    if c == q {
                        break;
                    }
                    if c == b'\\' {
                        let Some(&e) = bytes.get(i) else {
                            return Err("unterminated quoted string".into());
                        };
                        i += 1;
                        if q == b'\'' {
                            // Single quotes only escape the quote itself.
                            if e == b'\'' {
                                word.push(b'\'');
                            } else {
                                word.extend([b'\\', e]);
                            }
                            continue;
                        }
                        match e {
                            b'n' => word.push(b'\n'),
                            b'r' => word.push(b'\r'),
                            b't' => word.push(b'\t'),
                            b'x' => {
                                let hex = line.get(i..i + 2).ok_or("bad \\x escape")?;
                                word.push(
                                    u8::from_str_radix(hex, 16).map_err(|_| "bad \\x escape")?,
                                );
                                i += 2;
                            }
                            other => word.push(other),
                        }
                    } else {
                        word.push(c);
                    }
                }
                if bytes.get(i).is_some_and(|c| !c.is_ascii_whitespace()) {
                    return Err("closing quote must be followed by a space".into());
                }
            }
            _ => {
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    word.push(bytes[i]);
                    i += 1;
                }
            }
        }
        words.push(word);
    }
    Ok(words)
}

fn scalar(v: &redis::Value) -> Value {
    use redis::Value as V;
    match v {
        V::Nil => Value::Null,
        V::Int(i) => Value::Int(*i),
        V::Okay => Value::Text("OK".into()),
        V::SimpleString(s) => Value::Text(s.clone()),
        V::BulkString(b) => match std::str::from_utf8(b) {
            Ok(s) => Value::Text(s.to_owned()),
            Err(_) => Value::Bytes(b.clone()),
        },
        V::Double(d) => Value::Float(*d),
        V::Boolean(b) => Value::Bool(*b),
        V::VerbatimString { text, .. } => Value::Text(text.clone()),
        V::BigNumber(n) => Value::Text(String::from_utf8_lossy(&n_bytes(n)).into_owned()),
        V::ServerError(e) => Value::Text(format!("{e:?}")),
        V::Attribute { data, .. } => scalar(data),
        _ => Value::Null,
    }
}

fn n_bytes(n: &impl std::fmt::Debug) -> Vec<u8> {
    format!("{n:?}").into_bytes()
}

/// Leaves of an array in depth-first order.
fn flatten<'a>(v: &'a redis::Value, out: &mut Vec<&'a redis::Value>) {
    use redis::Value as V;
    match v {
        V::Array(items) | V::Set(items) | V::Push { data: items, .. } => {
            items.iter().for_each(|i| flatten(i, out));
        }
        V::Attribute { data, .. } => flatten(data, out),
        other => out.push(other),
    }
}

fn to_result(reply: &redis::Value, max_rows: usize) -> ResultSet {
    use redis::Value as V;
    match reply {
        V::Map(pairs) => {
            let mut rs =
                ResultSet { columns: vec!["key".into(), "value".into()], ..Default::default() };
            for (k, v) in pairs {
                if rs.rows.len() >= max_rows {
                    rs.truncated = true;
                    break;
                }
                let mut leaves = Vec::new();
                flatten(v, &mut leaves);
                let value = if leaves.len() == 1 {
                    scalar(leaves[0])
                } else {
                    Value::Text(
                        leaves.iter().map(|l| display(&scalar(l))).collect::<Vec<_>>().join(", "),
                    )
                };
                rs.rows.push(vec![scalar(k), value]);
            }
            rs
        }
        V::Array(_) | V::Set(_) | V::Push { .. } => {
            let mut leaves = Vec::new();
            flatten(reply, &mut leaves);
            let truncated = leaves.len() > max_rows;
            ResultSet {
                columns: vec!["value".into()],
                rows: leaves.into_iter().take(max_rows).map(|l| vec![scalar(l)]).collect(),
                truncated,
                ..Default::default()
            }
        }
        other => ResultSet {
            columns: vec!["result".into()],
            rows: vec![vec![scalar(other)]],
            ..Default::default()
        },
    }
}

fn display(v: &Value) -> String {
    text(v).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        split_command(s).unwrap().into_iter().map(|w| String::from_utf8(w).unwrap()).collect()
    }

    #[test]
    fn splits_like_a_shell() {
        assert_eq!(words("SET a 1"), ["SET", "a", "1"]);
        assert_eq!(words(r#"SET "my key" "va\"l\n""#), ["SET", "my key", "va\"l\n"]);
        assert_eq!(words("SET k 'it\\'s'"), ["SET", "k", "it's"]);
        assert_eq!(words(r#"SET k "\x41""#), ["SET", "k", "A"]);
        assert!(split_command("SET \"oops").is_err());
        assert!(split_command("SET \"a\"b").is_err());
    }

    #[test]
    fn url_encodes_credentials_and_picks_scheme() {
        let mut cfg: DataSourceConfig = toml::from_str("name='r'\ndialect='redis'").unwrap();
        cfg.host = Some("localhost".into());
        cfg.password = Some("p@ss/w:rd".into());
        assert_eq!(connection_url(&cfg).unwrap(), "redis://:p%40ss%2Fw%3Ard@localhost:6379/0");
        cfg.host = Some("cache.example.com".into());
        cfg.database = Some("3".into());
        cfg.user = Some("me".into());
        assert_eq!(
            connection_url(&cfg).unwrap(),
            "rediss://me:p%40ss%2Fw%3Ard@cache.example.com:6379/3"
        );
        cfg.database = Some("x".into());
        assert!(connection_url(&cfg).is_err());
    }

    #[test]
    fn replies_become_result_sets() {
        use redis::Value as V;
        let rs = to_result(
            &V::Array(vec![
                V::BulkString(b"0".to_vec()),
                V::Array(vec![V::BulkString(b"k".to_vec())]),
            ]),
            10,
        );
        assert_eq!(rs.rows.len(), 2);
        let rs = to_result(&V::Map(vec![(V::BulkString(b"f".to_vec()), V::Int(1))]), 10);
        assert_eq!(rs.columns, ["key", "value"]);
        assert_eq!(to_result(&V::Nil, 10).rows[0][0], Value::Null);
        assert!(to_result(&V::Array(vec![V::Int(1), V::Int(2)]), 1).truncated);
    }

    #[test]
    fn read_only_screen() {
        assert!(is_read_only("get"));
        assert!(!is_read_only("SET"));
        assert!(!is_read_only("FLUSHALL"));
    }
}
