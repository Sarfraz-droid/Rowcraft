//! Couchbase dialect (tier B, see SPEC.md §5) over the Query service's HTTP API
//! (`POST /query/service`, SQL++ / N1QL), with `reqwest` and rustls.
//!
//! Results are JSON documents. Object results become one column per key (in first-seen order);
//! other results go into a single `value` column. The explorer lists each `bucket.scope` as a
//! schema and each collection as a table; documents have no fixed columns, so none are listed.

use async_trait::async_trait;
use rowcraft_config::{DataSourceConfig, SslMode};
use rowcraft_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result,
    ResultSet, Value, split_statements,
};
use rowcraft_model::{Catalog, Schema, Table};
use serde_json::{Map, Value as Json};

#[derive(Debug, Default, Clone, Copy)]
pub struct CouchbaseDialect;

#[async_trait]
impl Dialect for CouchbaseDialect {
    fn id(&self) -> DialectId {
        DialectId("couchbase")
    }

    fn display_name(&self) -> &'static str {
        "Couchbase"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Document
    }

    fn default_port(&self) -> Option<u16> {
        Some(8093)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn connects(&self) -> bool {
        true
    }

    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut builder = reqwest::Client::builder().user_agent("Rowcraft");
        if cfg.ssl_mode == SslMode::Require {
            builder = builder.danger_accept_invalid_certs(true);
        }
        let client = builder.build().map_err(|e| Error::Connect(e.to_string()))?;
        let mut conn = CouchbaseConnection {
            client,
            url: query_url(cfg),
            user: cfg.user.clone().unwrap_or_default(),
            password: cfg.password.clone().unwrap_or_default(),
            read_only: cfg.read_only,
        };
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
        // Bucket rows have no `scope`; collection rows carry bucket, scope and name.
        let rs = conn
            .query(
                "select `bucket`, `scope`, `name` from system:keyspaces \
                 where `scope` is not missing order by `bucket`, `scope`, `name`",
            )
            .await?;
        let mut schemas: Vec<Schema> = Vec::new();
        for row in &rs.rows {
            let get = |i: usize| match row.get(i) {
                Some(Value::Text(s)) => Some(s.clone()),
                _ => None,
            };
            let (Some(bucket), Some(sc), Some(name)) = (get(0), get(1), get(2)) else { continue };
            let schema = format!("{bucket}.{sc}");
            if scope.schemas.as_ref().is_some_and(|only| !only.contains(&schema)) {
                continue;
            }
            let at = match schemas.iter().position(|s| s.name == schema) {
                Some(i) => i,
                None => {
                    schemas.push(Schema { name: schema, ..Schema::default() });
                    schemas.len() - 1
                }
            };
            schemas[at].tables.push(Table { name, ..Table::default() });
        }
        Ok(Catalog { database: None, default_schema: None, schemas })
    }
}

/// The Query service listens on 8093 (plain) and 18093 (TLS).
fn query_url(cfg: &DataSourceConfig) -> String {
    let port = cfg.port.unwrap_or(8093);
    let tls = match cfg.ssl_mode {
        SslMode::Disable => false,
        SslMode::Auto => port == 18093 || (!cfg.is_loopback_host() && port != 8093),
        _ => true,
    };
    let host = cfg.host.as_deref().unwrap_or("localhost");
    let host = if host.contains(':') { format!("[{host}]") } else { host.to_owned() };
    format!("{}://{host}:{port}/query/service", if tls { "https" } else { "http" })
}

#[derive(Debug)]
struct CouchbaseConnection {
    client: reqwest::Client,
    url: String,
    user: String,
    password: String,
    read_only: bool,
}

#[async_trait]
impl Connection for CouchbaseConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let mut last = ResultSet::default();
        for stmt in split_statements(statement) {
            let mut body = serde_json::json!({ "statement": stmt });
            if self.read_only {
                body["readonly"] = Json::Bool(true);
            }
            let response = self
                .client
                .post(&self.url)
                .basic_auth(&self.user, Some(&self.password))
                .json(&body)
                .send()
                .await
                .map_err(|e| Error::Query(e.to_string()))?;
            let status = response.status();
            let text = response.text().await.map_err(|e| Error::Query(e.to_string()))?;
            last = parse(status.as_u16(), &text, max_rows)?;
        }
        Ok(last)
    }
}

fn parse(status: u16, body: &str, max_rows: usize) -> Result<ResultSet> {
    let json: Json = match serde_json::from_str(body) {
        Ok(j) => j,
        // Auth failures and proxies answer in plain text.
        Err(_) if status >= 400 => {
            return Err(Error::Query(format!("HTTP {status}: {}", body.trim())));
        }
        Err(e) => return Err(Error::Query(format!("bad response: {e}"))),
    };
    if let Some(errors) = json["errors"].as_array().filter(|e| !e.is_empty()) {
        let msg = errors
            .iter()
            .map(|e| format!("{} {}", e["code"], e["msg"].as_str().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(Error::Query(msg));
    }
    if status >= 400 {
        return Err(Error::Query(format!("HTTP {status}")));
    }
    let results = json["results"].as_array().map(Vec::as_slice).unwrap_or_default();
    let mutations = json["metrics"]["mutationCount"].as_u64();
    if results.is_empty() {
        return Ok(ResultSet { rows_affected: mutations, ..ResultSet::default() });
    }

    // Column order: keys in first-seen order across all documents.
    let mut columns: Vec<String> = Vec::new();
    let all_objects = results.iter().all(Json::is_object);
    if all_objects {
        for doc in results {
            for key in doc.as_object().map(Map::keys).into_iter().flatten() {
                if !columns.contains(key) {
                    columns.push(key.clone());
                }
            }
        }
    } else {
        columns.push("value".into());
    }
    let rows = results
        .iter()
        .take(max_rows)
        .map(|doc| {
            if all_objects {
                columns.iter().map(|c| cell(doc.get(c))).collect()
            } else {
                vec![cell(Some(doc))]
            }
        })
        .collect();
    Ok(ResultSet { columns, rows, rows_affected: mutations, truncated: results.len() > max_rows })
}

fn cell(v: Option<&Json>) -> Value {
    match v {
        None | Some(Json::Null) => Value::Null,
        Some(Json::Bool(b)) => Value::Bool(*b),
        Some(Json::Number(n)) => {
            n.as_i64().map_or_else(|| Value::Float(n.as_f64().unwrap_or(f64::NAN)), Value::Int)
        }
        Some(Json::String(s)) => Value::Text(s.clone()),
        Some(other) => Value::Text(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(extra: &str) -> DataSourceConfig {
        toml::from_str(&format!("name='c'\ndialect='couchbase'\n{extra}")).unwrap()
    }

    #[test]
    fn url_scheme_follows_port() {
        assert_eq!(query_url(&cfg("")), "http://localhost:8093/query/service");
        assert_eq!(query_url(&cfg("host='h'\nport=18093")), "https://h:18093/query/service");
        assert_eq!(
            query_url(&cfg("host='h'\nssl_mode='disable'\nport=18093")),
            "http://h:18093/query/service"
        );
    }

    #[test]
    fn objects_become_columns_in_first_seen_order() {
        let body = r#"{"results":[{"a":1,"b":"x"},{"b":null,"c":[1,2]}],"status":"success"}"#;
        let rs = parse(200, body, 10).unwrap();
        assert_eq!(rs.columns, ["a", "b", "c"]);
        assert_eq!(rs.rows[0], vec![Value::Int(1), Value::Text("x".into()), Value::Null]);
        assert_eq!(rs.rows[1], vec![Value::Null, Value::Null, Value::Text("[1,2]".into())]);
    }

    #[test]
    fn scalars_use_a_value_column_and_mutations_are_reported() {
        let rs = parse(200, r#"{"results":[1,2,3]}"#, 2).unwrap();
        assert_eq!(rs.columns, ["value"]);
        assert!(rs.truncated);
        let rs = parse(200, r#"{"results":[],"metrics":{"mutationCount":4}}"#, 2).unwrap();
        assert_eq!(rs.rows_affected, Some(4));
    }

    #[test]
    fn errors_surface_code_and_message() {
        let e = parse(400, r#"{"errors":[{"code":3000,"msg":"syntax error"}]}"#, 10).unwrap_err();
        assert!(e.to_string().contains("3000 syntax error"));
        let e = parse(401, "Unauthorized", 10).unwrap_err();
        assert!(e.to_string().contains("HTTP 401"));
    }
}
