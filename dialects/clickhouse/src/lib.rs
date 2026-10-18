//! ClickHouse dialect (tier A, see SPEC.md §5) over the HTTP interface (`reqwest` + rustls).
//!
//! Each statement is one POST answered in `JSONCompact`; a script is split with
//! [`dg_dialect::split_statements`] because the HTTP interface runs one statement per request.

mod introspect;

use async_trait::async_trait;
use dg_config::{DataSourceConfig, SslMode};
use dg_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result,
    ResultSet, Value, split_statements,
};
use dg_model::Catalog;

#[derive(Debug, Default, Clone, Copy)]
pub struct ClickhouseDialect;

#[async_trait]
impl Dialect for ClickhouseDialect {
    fn id(&self) -> DialectId {
        DialectId("clickhouse")
    }

    fn display_name(&self) -> &'static str {
        "ClickHouse"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        Some(8123)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn connects(&self) -> bool {
        true
    }

    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .user_agent("Rowcraft")
            .build()
            .map_err(|e| Error::Connect(e.to_string()))?;
        let mut conn = ClickhouseConnection {
            client,
            base: base_url(cfg),
            user: cfg.user.clone().unwrap_or_else(|| "default".into()),
            password: cfg.password.clone().unwrap_or_default(),
            database: cfg.database.clone().filter(|d| !d.is_empty()),
            read_only: cfg.read_only,
        };
        // Surfaces bad credentials, a wrong database or an unreachable host now.
        conn.query_limited("select 1", 1).await.map_err(|e| match e {
            Error::Query(m) => Error::Connect(m),
            other => other,
        })?;
        Ok(Box::new(conn))
    }

    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        scope: &IntrospectScope,
    ) -> Result<Catalog> {
        introspect::introspect(conn, scope).await
    }
}

/// 8123 is the plain HTTP port and 8443 the TLS one; a remote host on any other port is assumed
/// to sit behind a TLS proxy.
fn base_url(cfg: &DataSourceConfig) -> String {
    let port = cfg.port.unwrap_or(8123);
    let tls = match cfg.ssl_mode {
        SslMode::Disable => false,
        SslMode::Auto => port == 8443 || (!cfg.is_loopback_host() && port != 8123),
        _ => true,
    };
    let host = cfg.host.as_deref().unwrap_or("localhost");
    let host = if host.contains(':') { format!("[{host}]") } else { host.to_owned() };
    format!("{}://{host}:{port}/", if tls { "https" } else { "http" })
}

#[derive(Debug)]
struct ClickhouseConnection {
    client: reqwest::Client,
    base: String,
    user: String,
    password: String,
    database: Option<String>,
    read_only: bool,
}

#[async_trait]
impl Connection for ClickhouseConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let mut last = ResultSet::default();
        for stmt in split_statements(statement) {
            last = self.run(stmt, max_rows).await?;
        }
        Ok(last)
    }
}

impl ClickhouseConnection {
    async fn run(&self, stmt: &str, max_rows: usize) -> Result<ResultSet> {
        let mut params: Vec<(&str, String)> = vec![("default_format", "JSONCompact".into())];
        if let Some(db) = &self.database {
            params.push(("database", db.clone()));
        }
        if self.read_only {
            params.push(("readonly", "1".into()));
        }
        if max_rows != usize::MAX {
            // Stops the server early; the block that crosses the limit is trimmed below.
            params.push(("max_result_rows", (max_rows + 1).to_string()));
            params.push(("result_overflow_mode", "break".into()));
        }
        let response = self
            .client
            .post(&self.base)
            .query(&params)
            .header("X-ClickHouse-User", &self.user)
            .header("X-ClickHouse-Key", &self.password)
            .body(stmt.to_owned())
            .send()
            .await
            .map_err(|e| Error::Query(e.to_string()))?;
        let status = response.status();
        let summary = response
            .headers()
            .get("x-clickhouse-summary")
            .and_then(|h| h.to_str().ok())
            .map(str::to_owned);
        let body = response.text().await.map_err(|e| Error::Query(e.to_string()))?;
        if !status.is_success() {
            return Err(Error::Query(body.trim().to_owned()));
        }
        parse(&body, summary.as_deref(), max_rows)
    }
}

fn parse(body: &str, summary: Option<&str>, max_rows: usize) -> Result<ResultSet> {
    if body.trim().is_empty() {
        // DDL and INSERT answer with an empty body.
        let written = summary
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| v.get("written_rows")?.as_str()?.parse().ok());
        return Ok(ResultSet { rows_affected: written, ..ResultSet::default() });
    }
    let json: serde_json::Value =
        serde_json::from_str(body).map_err(|e| Error::Query(format!("bad response: {e}")))?;
    let columns = json["meta"]
        .as_array()
        .map(|m| m.iter().map(|c| c["name"].as_str().unwrap_or_default().to_owned()).collect())
        .unwrap_or_default();
    let data = json["data"].as_array().map(Vec::as_slice).unwrap_or_default();
    Ok(ResultSet {
        columns,
        truncated: data.len() > max_rows,
        rows: data
            .iter()
            .take(max_rows)
            .map(|r| r.as_array().map_or_else(Vec::new, |r| r.iter().map(cell).collect()))
            .collect(),
        rows_affected: None,
    })
}

fn cell(v: &serde_json::Value) -> Value {
    use serde_json::Value as J;
    match v {
        J::Null => Value::Null,
        J::Bool(b) => Value::Bool(*b),
        J::Number(n) => {
            n.as_i64().map_or_else(|| Value::Float(n.as_f64().unwrap_or(f64::NAN)), Value::Int)
        }
        J::String(s) => Value::Text(s.clone()),
        other => Value::Text(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(extra: &str) -> DataSourceConfig {
        toml::from_str(&format!("name='c'\ndialect='clickhouse'\n{extra}")).unwrap()
    }

    #[test]
    fn picks_scheme_from_port_and_mode() {
        assert_eq!(base_url(&cfg("")), "http://localhost:8123/");
        assert_eq!(base_url(&cfg("host='h'\nport=8443")), "https://h:8443/");
        assert_eq!(base_url(&cfg("host='h'\nport=8123")), "http://h:8123/");
        assert_eq!(base_url(&cfg("host='h'\nport=443")), "https://h:443/");
        assert_eq!(base_url(&cfg("host='h'\nssl_mode='disable'\nport=8443")), "http://h:8443/");
    }

    #[test]
    fn parses_json_compact() {
        let body = r#"{"meta":[{"name":"a","type":"UInt8"},{"name":"b","type":"String"}],"data":[[1,"x"],[2,null],[3,"[1]"]],"rows":3}"#;
        let rs = parse(body, None, 2).unwrap();
        assert_eq!(rs.columns, ["a", "b"]);
        assert_eq!(
            rs.rows,
            [vec![Value::Int(1), Value::Text("x".into())], vec![Value::Int(2), Value::Null]]
        );
        assert!(rs.truncated);
    }

    #[test]
    fn empty_body_reports_written_rows() {
        let rs = parse("", Some(r#"{"read_rows":"0","written_rows":"3"}"#), 10).unwrap();
        assert_eq!(rs.rows_affected, Some(3));
        assert!(rs.columns.is_empty());
    }

    /// Serves each canned response once, returning the raw requests it saw.
    async fn fake_server(
        responses: Vec<&'static str>,
    ) -> (u16, tokio::task::JoinHandle<Vec<String>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let mut seen = Vec::new();
            for body in responses {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap();
                seen.push(String::from_utf8_lossy(&buf[..n]).into_owned());
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                sock.write_all(reply.as_bytes()).await.unwrap();
            }
            seen
        });
        (port, task)
    }

    #[tokio::test]
    async fn sends_credentials_database_and_statement() {
        let ok = r#"{"meta":[{"name":"1","type":"UInt8"}],"data":[[1]],"rows":1}"#;
        let (port, server) = fake_server(vec![ok, ok, ""]).await;
        let mut conn = ClickhouseDialect
            .connect(&cfg(&format!(
                "host='127.0.0.1'\nport={port}\nuser='ann'\ndatabase='app'\nread_only=true"
            )))
            .await
            .unwrap();
        let rs = conn
            .query_limited("select 1; create table t(a Int8) engine=Memory", 100)
            .await
            .unwrap();
        assert_eq!(rs.rows_affected, None);
        let seen = server.await.unwrap();
        let req = seen[1].to_lowercase();
        assert!(req.starts_with("post /?"));
        assert!(req.contains("database=app") && req.contains("readonly=1"));
        assert!(req.contains("x-clickhouse-user: ann"));
        assert!(req.ends_with("select 1"));
        assert!(seen[2].ends_with("create table t(a Int8) engine=Memory"));
    }

    #[tokio::test]
    async fn server_error_text_is_the_message() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let body = "Code: 516. DB::Exception: default: Authentication failed";
            let _ = sock
                .write_all(format!("HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes())
                .await;
        });
        let err = ClickhouseDialect
            .connect(&cfg(&format!("host='127.0.0.1'\nport={port}")))
            .await
            .unwrap_err();
        assert!(matches!(&err, Error::Connect(m) if m.contains("Authentication failed")), "{err}");
    }
}
