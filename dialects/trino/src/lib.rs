//! Trino and Presto dialects over their HTTP statement protocol (`POST /v1/statement`, then
//! follow `nextUri`), with `reqwest` and rustls. The two differ only in header prefix.
//!
//! * **Host / port**: the coordinator (default 8080; 8443 and anything remote use https).
//! * **User**: the principal (`X-Trino-User`). A password adds HTTP basic auth, which Trino only
//!   accepts over TLS.
//! * **Database**: `catalog` or `catalog/schema`. Without one the explorer lists every catalog
//!   except `system`. `USE catalog.schema` is honoured for the rest of the session.
//! * Every value arrives as text; arrays, maps and rows as JSON.

mod introspect;

use async_trait::async_trait;
use rowcraft_config::{DataSourceConfig, SslMode};
use rowcraft_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result,
    ResultSet, Value, split_statements,
};
use rowcraft_model::Catalog;
use serde_json::Value as Json;

/// What differs between the two servers.
#[derive(Debug, Clone, Copy)]
struct Flavor {
    id: &'static str,
    name: &'static str,
    /// Prefix of the `X-<prefix>-*` headers.
    prefix: &'static str,
}

const TRINO: Flavor = Flavor { id: "trino", name: "Trino", prefix: "Trino" };
const PRESTO: Flavor = Flavor { id: "presto", name: "Presto", prefix: "Presto" };

#[derive(Debug, Default, Clone, Copy)]
pub struct TrinoDialect;

#[derive(Debug, Default, Clone, Copy)]
pub struct PrestoDialect;

macro_rules! dialect {
    ($ty:ty, $flavor:expr) => {
        #[async_trait]
        impl Dialect for $ty {
            fn id(&self) -> DialectId {
                DialectId($flavor.id)
            }

            fn display_name(&self) -> &'static str {
                $flavor.name
            }

            fn family(&self) -> DialectFamily {
                DialectFamily::Sql
            }

            fn default_port(&self) -> Option<u16> {
                Some(8080)
            }

            fn capabilities(&self) -> Capabilities {
                Capabilities::default()
            }

            fn connects(&self) -> bool {
                true
            }

            async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
                connect($flavor, cfg).await
            }

            async fn introspect(
                &self,
                conn: &mut dyn Connection,
                scope: &IntrospectScope,
            ) -> Result<Catalog> {
                introspect::introspect(conn, scope).await
            }
        }
    };
}

dialect!(TrinoDialect, TRINO);
dialect!(PrestoDialect, PRESTO);

async fn connect(flavor: Flavor, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut builder = reqwest::Client::builder().user_agent("Rowcraft");
    if cfg.ssl_mode == SslMode::Require {
        builder = builder.danger_accept_invalid_certs(true);
    }
    let client = builder.build().map_err(|e| Error::Connect(e.to_string()))?;
    let (catalog, schema) = match cfg.database.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        None => (None, None),
        Some(d) => match d.split_once('/') {
            Some((c, s)) => (Some(c.to_owned()), Some(s.to_owned())),
            None => (Some(d.to_owned()), None),
        },
    };
    let mut conn = TrinoConnection {
        client,
        flavor,
        base: base_url(cfg),
        user: cfg.user.clone().filter(|u| !u.is_empty()).unwrap_or_else(|| "rowcraft".into()),
        password: cfg.password.clone().filter(|p| !p.is_empty()),
        catalog,
        schema,
        read_only: cfg.read_only,
    };
    conn.query_limited("select 1", 1).await.map_err(|e| match e {
        Error::Query(m) => Error::Connect(m),
        other => other,
    })?;
    Ok(Box::new(conn))
}

fn base_url(cfg: &DataSourceConfig) -> String {
    let port = cfg.port.unwrap_or(8080);
    let tls = match cfg.ssl_mode {
        SslMode::Disable => false,
        SslMode::Auto => port == 8443 || port == 443 || (!cfg.is_loopback_host() && port != 8080),
        _ => true,
    };
    let host = cfg.host.as_deref().unwrap_or("localhost");
    let host = if host.contains(':') { format!("[{host}]") } else { host.to_owned() };
    format!("{}://{host}:{port}", if tls { "https" } else { "http" })
}

#[derive(Debug)]
struct TrinoConnection {
    client: reqwest::Client,
    flavor: Flavor,
    base: String,
    user: String,
    password: Option<String>,
    catalog: Option<String>,
    schema: Option<String>,
    read_only: bool,
}

#[async_trait]
impl Connection for TrinoConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let mut last = ResultSet::default();
        for stmt in split_statements(statement) {
            if self.read_only && !is_read_only(stmt) {
                return Err(Error::Query(
                    "only read-only statements are allowed on a read-only data source".into(),
                ));
            }
            last = self.run(stmt, max_rows).await?;
        }
        Ok(last)
    }
}

fn is_read_only(stmt: &str) -> bool {
    let word = stmt.trim_start().split(|c: char| !c.is_ascii_alphabetic()).next().unwrap_or("");
    matches!(
        word.to_ascii_uppercase().as_str(),
        "SELECT" | "WITH" | "SHOW" | "DESCRIBE" | "DESC" | "EXPLAIN" | "USE" | "VALUES" | "TABLE"
    )
}

impl TrinoConnection {
    fn header(&self, name: &str) -> String {
        format!("X-{}-{name}", self.flavor.prefix)
    }

    fn request(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut req =
            req.header(self.header("User"), &self.user).header(self.header("Source"), "Rowcraft");
        if let Some(c) = &self.catalog {
            req = req.header(self.header("Catalog"), c);
        }
        if let Some(s) = &self.schema {
            req = req.header(self.header("Schema"), s);
        }
        if let Some(p) = &self.password {
            req = req.basic_auth(&self.user, Some(p));
        }
        req
    }

    async fn run(&mut self, stmt: &str, max_rows: usize) -> Result<ResultSet> {
        let mut response = self
            .request(self.client.post(format!("{}/v1/statement", self.base)))
            .body(stmt.to_owned())
            .send()
            .await
            .map_err(|e| Error::Query(e.to_string()))?;
        let mut rs = ResultSet::default();
        let mut polls = 0u32;
        loop {
            // `USE` answers through headers; later statements run in the new catalog/schema.
            if let Some(c) =
                response.headers().get(self.header("Set-Catalog")).and_then(|h| h.to_str().ok())
            {
                self.catalog = Some(c.to_owned());
            }
            if let Some(s) =
                response.headers().get(self.header("Set-Schema")).and_then(|h| h.to_str().ok())
            {
                self.schema = Some(s.to_owned());
            }
            let status = response.status();
            let text = response.text().await.map_err(|e| Error::Query(e.to_string()))?;
            let json: Json = serde_json::from_str(&text).map_err(|_| {
                Error::Query(format!(
                    "HTTP {status}: {}",
                    text.trim().chars().take(200).collect::<String>()
                ))
            })?;
            let next = absorb(&json, &mut rs, max_rows)?;
            let Some(next) = next else { return Ok(rs) };
            if rs.truncated {
                // Enough rows; cancel the query so the server stops working.
                let _ = self.request(self.client.delete(&next)).send().await;
                return Ok(rs);
            }
            // Early polls are tight, then back off while the query runs.
            polls += 1;
            tokio::time::sleep(std::time::Duration::from_millis(if polls < 5 { 20 } else { 100 }))
                .await;
            response = self
                .request(self.client.get(&next))
                .send()
                .await
                .map_err(|e| Error::Query(e.to_string()))?;
        }
    }
}

/// Merges one protocol page into `rs`; returns the `nextUri` to follow, if any.
fn absorb(page: &Json, rs: &mut ResultSet, max_rows: usize) -> Result<Option<String>> {
    if let Some(err) = page.get("error").filter(|e| !e.is_null()) {
        let name = err["errorName"].as_str().unwrap_or("ERROR");
        let msg = err["message"].as_str().unwrap_or("query failed");
        return Err(Error::Query(format!("{name}: {msg}")));
    }
    if rs.columns.is_empty() {
        if let Some(cols) = page["columns"].as_array() {
            rs.columns =
                cols.iter().map(|c| c["name"].as_str().unwrap_or_default().to_owned()).collect();
        }
    }
    for row in page["data"].as_array().map(Vec::as_slice).unwrap_or_default() {
        if rs.rows.len() >= max_rows {
            rs.truncated = true;
            break;
        }
        rs.rows.push(row.as_array().map(|r| r.iter().map(cell).collect()).unwrap_or_default());
    }
    if let Some(n) = page["updateCount"].as_u64() {
        rs.rows_affected = Some(n);
    }
    Ok(page["nextUri"].as_str().map(str::to_owned))
}

fn cell(v: &Json) -> Value {
    match v {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => {
            n.as_i64().map_or_else(|| Value::Float(n.as_f64().unwrap_or(f64::NAN)), Value::Int)
        }
        Json::String(s) => Value::Text(s.clone()),
        other => Value::Text(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(extra: &str) -> DataSourceConfig {
        toml::from_str(&format!("name='t'\ndialect='trino'\n{extra}")).unwrap()
    }

    #[test]
    fn scheme_follows_port_and_mode() {
        assert_eq!(base_url(&cfg("")), "http://localhost:8080");
        assert_eq!(base_url(&cfg("host='h'\nport=8443")), "https://h:8443");
        assert_eq!(base_url(&cfg("host='h'\nport=8080")), "http://h:8080");
        assert_eq!(base_url(&cfg("host='h'\nport=9000")), "https://h:9000");
        assert_eq!(base_url(&cfg("host='h'\nport=8443\nssl_mode='disable'")), "http://h:8443");
    }

    #[test]
    fn pages_accumulate_columns_and_rows() {
        let mut rs = ResultSet::default();
        assert_eq!(
            absorb(&json!({"nextUri": "http://x/1", "stats": {}}), &mut rs, 10).unwrap().as_deref(),
            Some("http://x/1")
        );
        assert!(rs.columns.is_empty());
        absorb(&json!({"columns": [{"name": "a"}, {"name": "b"}], "data": [[1, "x"], [2, null]], "nextUri": "http://x/2"}), &mut rs, 3).unwrap();
        let last = absorb(&json!({"data": [[3, [1, 2]], [4, "y"]]}), &mut rs, 3).unwrap();
        assert!(last.is_none());
        assert_eq!(rs.columns, ["a", "b"]);
        assert_eq!(rs.rows.len(), 3);
        assert_eq!(rs.rows[2][1], Value::Text("[1,2]".into()));
        assert!(rs.truncated);
    }

    #[test]
    fn dml_reports_update_count_and_errors_are_named() {
        let mut rs = ResultSet::default();
        absorb(&json!({"updateType": "INSERT", "updateCount": 5}), &mut rs, 10).unwrap();
        assert_eq!(rs.rows_affected, Some(5));
        let e = absorb(&json!({"error": {"errorName": "TABLE_NOT_FOUND", "message": "line 1:15: Table 'x' does not exist"}}), &mut ResultSet::default(), 10).unwrap_err();
        assert!(e.to_string().contains("TABLE_NOT_FOUND: line 1:15"));
    }

    #[test]
    fn read_only_screen() {
        assert!(is_read_only("select 1"));
        assert!(is_read_only("SHOW CATALOGS"));
        assert!(!is_read_only("insert into t values (1)"));
        assert!(!is_read_only("drop table t"));
    }
}
