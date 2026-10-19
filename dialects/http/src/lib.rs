//! Native HTTP connectors: Neo4j Query API v2 and GraphQL over HTTP.
use async_trait::async_trait;
use reqwest::{Client, Url};
use rowcraft_config::{DataSourceConfig, SslMode};
use rowcraft_dialect::{
    Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result, ResultSet, Value,
};
use rowcraft_model::{Catalog, Column, Schema, Table};
use serde_json::{Value as Json, json};
use std::{fmt, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Neo4j,
    Graphql,
}
#[derive(Debug, Clone, Copy)]
pub struct HttpDialect {
    kind: Kind,
}
impl HttpDialect {
    pub fn neo4j() -> Self {
        Self { kind: Kind::Neo4j }
    }
    pub fn graphql() -> Self {
        Self { kind: Kind::Graphql }
    }
}
#[async_trait]
impl Dialect for HttpDialect {
    fn id(&self) -> DialectId {
        DialectId(if self.kind == Kind::Neo4j { "neo4j" } else { "graphql" })
    }
    fn display_name(&self) -> &'static str {
        if self.kind == Kind::Neo4j { "Neo4j" } else { "GraphQL" }
    }
    fn family(&self) -> DialectFamily {
        if self.kind == Kind::Neo4j { DialectFamily::Graph } else { DialectFamily::Api }
    }
    fn default_port(&self) -> Option<u16> {
        Some(if self.kind == Kind::Neo4j { 7474 } else { 4000 })
    }
    fn connects(&self) -> bool {
        true
    }
    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let endpoint = endpoint(cfg, self.kind)?;
        let client = Client::builder()
            .timeout(Duration::from_secs(45))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| Error::Connect("Could not create HTTP client".into()))?;
        let mut conn = HttpConnection {
            client,
            endpoint,
            kind: self.kind,
            user: cfg.user.clone(),
            password: cfg.password.clone(),
            read_only: cfg.read_only,
        };
        conn.query_limited(
            if self.kind == Kind::Neo4j { "RETURN 1 AS connected" } else { "query { __typename }" },
            1,
        )
        .await?;
        Ok(Box::new(conn))
    }
    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        _scope: &IntrospectScope,
    ) -> Result<Catalog> {
        if self.kind == Kind::Graphql {
            let result = conn.query("query { __schema { queryType { name fields { name args { name defaultValue type { kind name ofType { kind name } } } type { kind name ofType { kind name ofType { kind name } } } } } types { name fields { name args { name defaultValue type { kind } } type { kind name ofType { kind name ofType { kind name } } } } } } }").await?;
            let raw = result.rows.first().and_then(|row| row.first()).and_then(|value| if let Value::Text(text) = value { serde_json::from_str::<Json>(text).ok() } else { None }).ok_or_else(|| Error::Query("GraphQL introspection returned no schema. The endpoint may disable introspection.".into()))?;
            return graphql_catalog(&raw);
        }
        let labels = conn.query("MATCH (n) WITH n LIMIT 1000 UNWIND labels(n) AS label RETURN DISTINCT label LIMIT 1000").await?;
        let mut tables: Vec<Table> = labels
            .rows
            .iter()
            .filter_map(|row| match row.first() {
                Some(Value::Text(label)) => Some(Table {
                    name: label.clone(),
                    comment: Some("Node labels sampled from up to 1,000 nodes.".into()),
                    ..Table::default()
                }),
                _ => None,
            })
            .collect();
        let fields = conn.query("MATCH (n) WITH n LIMIT 1000 UNWIND labels(n) AS label UNWIND keys(n) AS property RETURN DISTINCT label, property LIMIT 2000").await?;
        for row in fields.rows {
            if let (Some(Value::Text(label)), Some(Value::Text(property))) =
                (row.first(), row.get(1))
            {
                if let Some(table) = tables.iter_mut().find(|table| table.name == *label) {
                    if !table.columns.iter().any(|column| column.name == *property) {
                        table.columns.push(Column {
                            name: property.clone(),
                            native_type: "dynamic".into(),
                            ..Column::default()
                        });
                    }
                }
            }
        }
        Ok(Catalog {
            database: None,
            default_schema: Some("graph".into()),
            schemas: vec![Schema { name: "graph".into(), tables, ..Schema::default() }],
        })
    }
}

struct HttpConnection {
    client: Client,
    endpoint: Url,
    kind: Kind,
    user: Option<String>,
    password: Option<String>,
    read_only: bool,
}
impl fmt::Debug for HttpConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpConnection")
            .field("kind", &self.kind)
            .field("read_only", &self.read_only)
            .finish_non_exhaustive()
    }
}
fn endpoint(cfg: &DataSourceConfig, kind: Kind) -> Result<Url> {
    let host = cfg.host.as_deref().unwrap_or("localhost");
    let scheme = if matches!(cfg.ssl_mode, SslMode::Disable)
        || (matches!(cfg.ssl_mode, SslMode::Auto)
            && matches!(host, "localhost" | "127.0.0.1" | "::1"))
    {
        "http"
    } else {
        "https"
    };
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    let fallback = format!(
        "{scheme}://{host}:{}",
        cfg.port.unwrap_or(if kind == Kind::Neo4j { 7474 } else { 4000 })
    );
    let mut url = Url::parse(cfg.url.as_deref().filter(|s| !s.is_empty()).unwrap_or(&fallback))
        .map_err(|_| Error::Connect("Enter a valid HTTP endpoint URL.".into()))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Connect("Use an HTTP(S) endpoint without embedded credentials or query parameters. Put credentials in the password field.".into()));
    }
    if url.path() == "/" {
        if kind == Kind::Graphql {
            url.set_path("/graphql");
        } else {
            url.path_segments_mut()
                .map_err(|_| Error::Connect("Invalid endpoint".into()))?
                .clear()
                .extend([
                    "db",
                    cfg.database.as_deref().filter(|s| !s.is_empty()).unwrap_or("neo4j"),
                    "query",
                    "v2",
                ]);
        }
    }
    Ok(url)
}
fn read_only(query: &str, kind: Kind) -> bool {
    // Conservative, fail-closed screening supplements server-side read credentials.
    let tokens: Vec<String> = query
        .split(|c: char| !c.is_ascii_alphabetic() && c != '_')
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_uppercase)
        .collect();
    if kind == Kind::Graphql {
        let first = query
            .lines()
            .map(|line| line.split('#').next().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n");
        return (first.trim_start().starts_with('{') || first.trim_start().starts_with("query"))
            && !tokens.iter().any(|s| ["MUTATION", "SUBSCRIPTION"].contains(&s.as_str()));
    }
    matches!(
        tokens.first().map(String::as_str),
        Some("MATCH" | "OPTIONAL" | "RETURN" | "WITH" | "EXPLAIN" | "PROFILE")
    ) && !tokens.iter().any(|s| {
        [
            "CREATE", "MERGE", "SET", "DELETE", "DETACH", "REMOVE", "DROP", "ALTER", "INSERT",
            "FOREACH", "CALL", "LOAD", "USE", "GRANT", "DENY", "REVOKE",
        ]
        .contains(&s.as_str())
    })
}
#[async_trait]
impl Connection for HttpConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let body = request_body(statement, self.kind, self.read_only)?;
        let mut request = self
            .client
            .post(self.endpoint.clone())
            .header("Accept", "application/json")
            .json(&body);
        if let Some(password) = self.password.as_deref().filter(|s| !s.is_empty()) {
            request = if let Some(user) = self.user.as_deref().filter(|s| !s.is_empty()) {
                request.basic_auth(user, Some(password))
            } else {
                request.bearer_auth(password)
            };
        }
        let mut response = request.send().await.map_err(|_| {
            Error::Query(
                "Could not reach the HTTP endpoint. Check its URL and connection settings.".into(),
            )
        })?;
        if !response.status().is_success() {
            return Err(Error::Query(format!(
                "HTTP endpoint returned {}. Check the endpoint, credentials, and service availability.",
                response.status().as_u16()
            )));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) =
            response.chunk().await.map_err(|_| Error::Query("HTTP response interrupted".into()))?
        {
            if bytes.len() + chunk.len() > 8_000_000 {
                return Err(Error::Query(
                    "Response exceeds 8 MB. Add a limit or narrow the requested fields.".into(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let data: Json = serde_json::from_slice(&bytes)
            .map_err(|_| Error::Query("The endpoint did not return valid JSON.".into()))?;
        decode(&data, self.kind, max_rows)
    }
}
fn request_body(statement: &str, kind: Kind, restricted: bool) -> Result<Json> {
    let mut body = if statement.trim_start().starts_with('{') {
        serde_json::from_str::<Json>(statement)
            .ok()
            .filter(|v| v.get(if kind == Kind::Neo4j { "statement" } else { "query" }).is_some())
            .unwrap_or_else(|| {
                if kind == Kind::Neo4j {
                    json!({"statement":statement})
                } else {
                    json!({"query":statement})
                }
            })
    } else if kind == Kind::Neo4j {
        json!({"statement":statement})
    } else {
        json!({"query":statement})
    };
    let key = if kind == Kind::Neo4j { "statement" } else { "query" };
    let query =
        body[key].as_str().ok_or_else(|| Error::Query(format!("{key} must be a string")))?;
    if restricted && !read_only(query, kind) {
        return Err(Error::Query("This connection permits read queries only. Mutations, subscriptions, procedures, and unrecognized query forms are blocked.".into()));
    }
    // Never forward caller-supplied routing, auth, transaction, or URL fields.
    let mut allowed = json!({key:query});
    for field in
        if kind == Kind::Neo4j { &["parameters"][..] } else { &["variables", "operationName"][..] }
    {
        if let Some(value) = body.get_mut(*field) {
            allowed[*field] = value.take();
        }
    }
    Ok(allowed)
}
fn cell(value: &Json) -> Value {
    match value {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => n.as_i64().map(Value::Int).unwrap_or_else(|| {
            n.as_f64().map(Value::Float).unwrap_or_else(|| Value::Text(n.to_string()))
        }),
        Json::String(s) => Value::Text(s.clone()),
        _ => Value::Text(value.to_string()),
    }
}
fn decode(body: &Json, kind: Kind, limit: usize) -> Result<ResultSet> {
    if let Some(errors) =
        body.get("errors").and_then(Json::as_array).filter(|errors| !errors.is_empty())
    {
        return Err(Error::Query(
            errors
                .iter()
                .filter_map(|error| error.get("message").and_then(Json::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }
    if kind == Kind::Neo4j {
        let columns = body
            .pointer("/data/fields")
            .and_then(Json::as_array)
            .ok_or_else(|| Error::Query("Neo4j response is missing data.fields.".into()))?
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_owned())
            .collect();
        let values = body
            .pointer("/data/values")
            .and_then(Json::as_array)
            .ok_or_else(|| Error::Query("Neo4j response is missing data.values.".into()))?;
        return Ok(ResultSet {
            columns,
            rows: values
                .iter()
                .take(limit)
                .map(|row| {
                    row.as_array().map(|row| row.iter().map(cell).collect()).unwrap_or_default()
                })
                .collect(),
            truncated: values.len() > limit,
            ..ResultSet::default()
        });
    }
    let data =
        body.get("data").ok_or_else(|| Error::Query("GraphQL response is missing data.".into()))?;
    if let Some(schema) = data.get("__schema") {
        return Ok(ResultSet {
            columns: vec!["__schema".into()],
            rows: vec![vec![Value::Text(schema.to_string())]],
            ..ResultSet::default()
        });
    }
    let root = data
        .as_object()
        .filter(|root| root.len() == 1)
        .and_then(|root| root.values().next())
        .unwrap_or(data);
    let records: Vec<&Json> =
        root.as_array().map(|items| items.iter().collect()).unwrap_or_else(|| vec![root]);
    let mut columns = Vec::new();
    for record in records.iter().take(limit) {
        if let Some(object) = record.as_object() {
            for key in object.keys() {
                if !columns.contains(key) {
                    columns.push(key.clone());
                }
            }
        }
    }
    if columns.is_empty() {
        columns.push("value".into());
    }
    Ok(ResultSet {
        rows: records
            .iter()
            .take(limit)
            .map(|record| {
                if record.is_object() {
                    columns.iter().map(|key| cell(&record[key])).collect()
                } else {
                    vec![cell(record)]
                }
            })
            .collect(),
        columns,
        truncated: records.len() > limit,
        ..ResultSet::default()
    })
}
fn graphql_catalog(schema: &Json) -> Result<Catalog> {
    let query = schema
        .get("queryType")
        .filter(|q| q.is_object())
        .ok_or_else(|| Error::Query("GraphQL introspection has no query type.".into()))?;
    let name = query["name"].as_str().unwrap_or("Query");
    let types = schema["types"].as_array().cloned().unwrap_or_default();
    let tables = query["fields"].as_array().cloned().unwrap_or_default().iter().map(|field| {
        let field_name = field["name"].as_str().unwrap_or_default();
        let mut ty = &field["type"];
        while ty.get("ofType").is_some_and(Json::is_object) { ty = &ty["ofType"]; }
        let target = types.iter().find(|t| t["name"] == ty["name"]);
        let columns: Vec<Column> = target.and_then(|t| t["fields"].as_array()).into_iter().flatten().filter_map(|f| {
            let mut child = &f["type"]; while child.get("ofType").is_some_and(Json::is_object) {child = &child["ofType"];}
            if !matches!(child["kind"].as_str(), Some("SCALAR" | "ENUM")) || f["args"].as_array().is_some_and(|args| args.iter().any(|a| a["type"]["kind"] == "NON_NULL" && a["defaultValue"].is_null())) { return None; }
            Some(Column {name:f["name"].as_str()?.into(),native_type:child["name"].as_str().unwrap_or("JSON").into(),..Column::default()})
        }).take(12).collect();
        let selection = if matches!(ty["kind"].as_str(), Some("SCALAR" | "ENUM")) {String::new()} else {format!(" {{ {} }}", if columns.is_empty() {"__typename".into()} else {columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(" ")})};
        let required = field["args"].as_array().is_some_and(|args| args.iter().any(|a| a["type"]["kind"] == "NON_NULL" && a["defaultValue"].is_null()));
        Table {name:field_name.into(), columns, comment:Some(if required {"Requires GraphQL arguments; write this query with variables in the GraphQL console.".into()} else {format!("query {{ {field_name}{selection} }}")}),..Table::default()}
    }).collect();
    Ok(Catalog {
        database: Some(name.into()),
        default_schema: Some(name.into()),
        schemas: vec![Schema { name: name.into(), tables, ..Schema::default() }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graphql_and_cypher_requests_and_guards() {
        assert_eq!(
            request_body("query { __typename }", Kind::Graphql, true).unwrap()["query"],
            "query { __typename }"
        );
        assert!(request_body("mutation { deleteAll }", Kind::Graphql, true).is_err());
        assert!(
            request_body(
                r#"{"query":"query A { a } mutation B { deleteAll }","operationName":"B"}"#,
                Kind::Graphql,
                true
            )
            .is_err()
        );
        assert!(request_body("MATCH(n) DELETE n", Kind::Neo4j, true).is_err());
        assert!(request_body("CALL apoc.cypher.run('CREATE(n)',{})", Kind::Neo4j, true).is_err());
        assert_eq!(
            request_body("MATCH(n) RETURN n LIMIT 5", Kind::Neo4j, true).unwrap()["statement"],
            "MATCH(n) RETURN n LIMIT 5"
        );
    }
    #[test]
    fn decodes_nodes_documents_and_errors() {
        let result = decode(&json!({"data":{"fields":["n"],"values":[[{"labels":["Person"],"properties":{"name":"A"}}]]}}),Kind::Neo4j,10).unwrap();
        assert_eq!(result.columns, ["n"]);
        let result = decode(
            &json!({"data":{"users":[{"id":1,"profile":{"name":"A"}},{"id":2}]}}),
            Kind::Graphql,
            1,
        )
        .unwrap();
        assert!(result.truncated);
        assert_eq!(result.columns, ["id", "profile"]);
        assert!(
            decode(&json!({"errors":[{"message":"Permission denied"}]}), Kind::Neo4j, 1).is_err()
        );
    }
    #[test]
    fn endpoint_uses_full_path_and_rejects_embedded_credentials() {
        let cfg = DataSourceConfig { database: Some("my graph".into()), ..Default::default() };
        assert_eq!(endpoint(&cfg, Kind::Neo4j).unwrap().path(), "/db/my%20graph/query/v2");
        let cfg = DataSourceConfig {
            url: Some("https://example.test/custom/graphql".into()),
            ..Default::default()
        };
        assert_eq!(endpoint(&cfg, Kind::Graphql).unwrap().path(), "/custom/graphql");
        let cfg = DataSourceConfig {
            url: Some("https://u:p@example.test/graphql".into()),
            ..Default::default()
        };
        assert!(endpoint(&cfg, Kind::Graphql).is_err());
    }

    #[test]
    fn graphql_variables_are_preserved_and_transport_overrides_are_removed() {
        let body = request_body(r#"{"query":"query User($id: ID!) { user(id: $id) { id } }","variables":{"id":"001"},"operationName":"User","endpoint":"https://other.test","headers":{"Authorization":"x"}}"#, Kind::Graphql, true).unwrap();
        assert_eq!(body["variables"]["id"], "001");
        assert_eq!(body["operationName"], "User");
        assert!(body.get("endpoint").is_none());
        assert!(body.get("headers").is_none());
    }

    #[test]
    fn graphql_previews_select_scalar_fields_and_require_explicit_arguments() {
        let catalog = graphql_catalog(&json!({
            "queryType":{"name":"Query","fields":[
                {"name":"users","args":[],"type":{"kind":"LIST","ofType":{"kind":"OBJECT","name":"User"}}},
                {"name":"user","args":[{"name":"id","defaultValue":null,"type":{"kind":"NON_NULL","ofType":{"kind":"SCALAR","name":"ID"}}}],"type":{"kind":"OBJECT","name":"User"}}
            ]},
            "types":[{"name":"User","fields":[
                {"name":"id","args":[],"type":{"kind":"SCALAR","name":"ID"}},
                {"name":"friends","args":[],"type":{"kind":"LIST","ofType":{"kind":"OBJECT","name":"User"}}},
                {"name":"privateField","args":[{"defaultValue":null,"type":{"kind":"NON_NULL"}}],"type":{"kind":"SCALAR","name":"String"}}
            ]}]
        })).unwrap();
        let tables = &catalog.schemas[0].tables;
        assert_eq!(tables[0].comment.as_deref(), Some("query { users { id } }"));
        assert_eq!(tables[0].columns.len(), 1);
        assert!(tables[1].comment.as_ref().unwrap().contains("Requires GraphQL arguments"));
        assert!(!tables[1].comment.as_ref().unwrap().starts_with("query {"));
    }
}
