//! Amazon DynamoDB dialect (tier A, see SPEC.md §5): PartiQL (`ExecuteStatement`) over the
//! DynamoDB JSON API, signed with AWS Signature V4, through `reqwest` and rustls. The AWS SDK is
//! not used, so the dependency tree stays small.
//!
//! Connection fields:
//! * **User / password**: access key id and secret key (the password comes from the data source's
//!   password variable). When User is empty, `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` apply.
//!   A session token comes from the Advanced property `session_token` or `AWS_SESSION_TOKEN`.
//! * **Database**: the region (`eu-west-1`); falls back to the `region` property, `AWS_REGION`.
//! * **Host / port**: optional endpoint override, e.g. `localhost` and `8000` for DynamoDB Local.
//!
//! Besides PartiQL the console accepts `show tables` and `describe <table>` (key schema and
//! indexes), which the explorer uses to list the region's tables with their key attributes.

mod introspect;
mod sigv4;

use async_trait::async_trait;
use base64::Engine;
use rowcraft_config::DataSourceConfig;
use rowcraft_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result,
    ResultSet, Value, split_statements,
};
use rowcraft_model::Catalog;
use serde_json::{Value as Json, json};

#[derive(Debug, Default, Clone, Copy)]
pub struct DynamoDialect;

#[async_trait]
impl Dialect for DynamoDialect {
    fn id(&self) -> DialectId {
        DialectId("dynamo")
    }

    fn display_name(&self) -> &'static str {
        "Amazon DynamoDB"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::KeyValue
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
        let conn = DynamoConnection {
            client,
            target: Target::from_config(cfg)?,
            creds: Credentials::from_config(cfg)?,
            read_only: cfg.read_only,
        };
        // Bad keys, region or endpoint all fail here instead of on the first real query.
        conn.call("ListTables", json!({ "Limit": 1 })).await.map_err(|e| match e {
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

#[derive(Debug, Clone)]
pub(crate) struct Credentials {
    access_key: String,
    secret_key: String,
    session_token: Option<String>,
}

impl Credentials {
    fn from_config(cfg: &DataSourceConfig) -> Result<Self> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let access_key = cfg
            .user
            .clone()
            .filter(|u| !u.is_empty())
            .or_else(|| env("AWS_ACCESS_KEY_ID"))
            .ok_or_else(|| {
                Error::Connect("set User to an access key id (or export AWS_ACCESS_KEY_ID)".into())
            })?;
        let secret_key = cfg
            .password
            .clone()
            .filter(|p| !p.is_empty())
            .or_else(|| env("AWS_SECRET_ACCESS_KEY"))
            .ok_or_else(|| {
                Error::Connect(
                    "set the password variable to the secret key (or export AWS_SECRET_ACCESS_KEY)"
                        .into(),
                )
            })?;
        let session_token = cfg
            .properties
            .get("session_token")
            .cloned()
            .filter(|t| !t.is_empty())
            .or_else(|| env("AWS_SESSION_TOKEN"));
        Ok(Self { access_key, secret_key, session_token })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Target {
    pub region: String,
    /// `https://dynamodb.<region>.amazonaws.com` or the override.
    pub url: String,
    /// Value of the `Host` header the signature covers.
    pub host: String,
}

impl Target {
    fn from_config(cfg: &DataSourceConfig) -> Result<Self> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let region = cfg
            .database
            .clone()
            .filter(|d| !d.is_empty())
            .or_else(|| cfg.properties.get("region").cloned().filter(|r| !r.is_empty()))
            .or_else(|| env("AWS_REGION"))
            .or_else(|| env("AWS_DEFAULT_REGION"))
            .ok_or_else(|| {
                Error::Connect("set Database to the AWS region (e.g. eu-west-1)".into())
            })?;
        let (url, host) = match cfg.host.as_deref().map(str::trim).filter(|h| !h.is_empty()) {
            None => {
                let host = format!("dynamodb.{region}.amazonaws.com");
                (format!("https://{host}"), host)
            }
            Some(h) if h.starts_with("http://") || h.starts_with("https://") => {
                let bare = h.trim_end_matches('/');
                (bare.to_owned(), bare.split_once("://").map_or(bare, |(_, r)| r).to_owned())
            }
            Some(h) => {
                let host = cfg.port.map_or_else(|| h.to_owned(), |p| format!("{h}:{p}"));
                let tls = match cfg.ssl_mode {
                    rowcraft_config::SslMode::Disable => false,
                    rowcraft_config::SslMode::Auto => !cfg.is_loopback_host(),
                    _ => true,
                };
                (format!("{}://{host}", if tls { "https" } else { "http" }), host)
            }
        };
        Ok(Self { region, url, host })
    }
}

#[derive(Debug)]
pub(crate) struct DynamoConnection {
    client: reqwest::Client,
    target: Target,
    creds: Credentials,
    read_only: bool,
}

impl DynamoConnection {
    /// One signed API call, e.g. `call("ExecuteStatement", json!({...}))`.
    pub(crate) async fn call(&self, operation: &str, body: Json) -> Result<Json> {
        let payload = body.to_string();
        let target = format!("DynamoDB_20120810.{operation}");
        let signed = sigv4::sign(
            &self.creds.access_key,
            &self.creds.secret_key,
            self.creds.session_token.as_deref(),
            &self.target.region,
            &self.target.host,
            &target,
            payload.as_bytes(),
            sigv4::now(),
        );
        let mut req = self
            .client
            .post(&self.target.url)
            .header("Content-Type", "application/x-amz-json-1.0")
            .header("X-Amz-Target", &target)
            .header("X-Amz-Date", &signed.amz_date)
            .header("Authorization", &signed.authorization);
        if let Some(token) = &self.creds.session_token {
            req = req.header("X-Amz-Security-Token", token);
        }
        let response = req.body(payload).send().await.map_err(|e| Error::Query(e.to_string()))?;
        let status = response.status();
        let text = response.text().await.map_err(|e| Error::Query(e.to_string()))?;
        if status.is_success() {
            return serde_json::from_str(&text)
                .map_err(|e| Error::Query(format!("bad response: {e}")));
        }
        Err(Error::Query(api_error(status.as_u16(), &text)))
    }
}

/// `{"__type":"com.amazonaws.dynamodb.v20120810#ResourceNotFoundException","message":"…"}`
fn api_error(status: u16, body: &str) -> String {
    match serde_json::from_str::<Json>(body) {
        Ok(j) => {
            let kind = j["__type"].as_str().map_or("", |t| t.rsplit('#').next().unwrap_or(t));
            let message =
                j["message"].as_str().or_else(|| j["Message"].as_str()).unwrap_or_default();
            format!("{kind}: {message}").trim_matches([':', ' ']).to_owned()
        }
        Err(_) => format!("HTTP {status}: {}", body.trim().chars().take(200).collect::<String>()),
    }
}

#[async_trait]
impl Connection for DynamoConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let mut last = ResultSet::default();
        for stmt in split_statements(statement) {
            if self.read_only && !is_read_only(stmt) {
                return Err(Error::Query(
                    "only SELECT is allowed on a read-only data source".into(),
                ));
            }
            last = self.run(stmt, max_rows).await?;
        }
        Ok(last)
    }
}

fn is_read_only(stmt: &str) -> bool {
    stmt.trim_start().get(..6).is_some_and(|w| w.eq_ignore_ascii_case("select"))
}

impl DynamoConnection {
    async fn run(&self, stmt: &str, max_rows: usize) -> Result<ResultSet> {
        let words: Vec<&str> = stmt.split_whitespace().collect();
        match words.as_slice() {
            [a, b] if a.eq_ignore_ascii_case("show") && b.eq_ignore_ascii_case("tables") => {
                return self.show_tables(max_rows).await;
            }
            [a, name] if a.eq_ignore_ascii_case("describe") => {
                return self
                    .describe(name.trim_matches(|c| c == '"' || c == '\'' || c == ';'))
                    .await;
            }
            _ => {}
        }
        let mut items: Vec<Json> = Vec::new();
        let mut token: Option<String> = None;
        let mut truncated = false;
        loop {
            let mut body = json!({ "Statement": stmt });
            if let Some(t) = &token {
                body["NextToken"] = Json::String(t.clone());
            }
            let reply = self.call("ExecuteStatement", body).await?;
            for item in reply["Items"].as_array().map(Vec::as_slice).unwrap_or_default() {
                if items.len() >= max_rows {
                    truncated = true;
                    break;
                }
                items.push(item.clone());
            }
            token = reply["NextToken"].as_str().map(str::to_owned);
            if truncated || token.is_none() {
                truncated = truncated || token.is_some() && items.len() >= max_rows;
                break;
            }
            if items.len() >= max_rows {
                truncated = true;
                break;
            }
        }
        Ok(items_to_result(&items, truncated))
    }
}

impl DynamoConnection {
    async fn show_tables(&self, max_rows: usize) -> Result<ResultSet> {
        let mut names: Vec<Value> = Vec::new();
        let mut start: Option<String> = None;
        let mut truncated = false;
        loop {
            let mut body = json!({ "Limit": 100 });
            if let Some(t) = &start {
                body["ExclusiveStartTableName"] = Json::String(t.clone());
            }
            let reply = self.call("ListTables", body).await?;
            for n in reply["TableNames"].as_array().map(Vec::as_slice).unwrap_or_default() {
                if names.len() >= max_rows {
                    truncated = true;
                    break;
                }
                names.push(Value::Text(n.as_str().unwrap_or_default().to_owned()));
            }
            start = reply["LastEvaluatedTableName"].as_str().map(str::to_owned);
            if start.is_none() || truncated {
                break;
            }
        }
        Ok(ResultSet {
            columns: vec!["name".into()],
            rows: names.into_iter().map(|n| vec![n]).collect(),
            truncated,
            ..ResultSet::default()
        })
    }

    /// Rows of `(kind, name, type, detail)`: `attribute` (declared key attribute and its type),
    /// `key` (HASH or RANGE), `index` (GSI or LSI, with its key attributes in `detail`).
    async fn describe(&self, table: &str) -> Result<ResultSet> {
        let reply = self.call("DescribeTable", json!({ "TableName": table })).await?;
        let t = &reply["Table"];
        let text = |s: &str| Value::Text(s.to_owned());
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for a in t["AttributeDefinitions"].as_array().map(Vec::as_slice).unwrap_or_default() {
            rows.push(vec![
                text("attribute"),
                text(a["AttributeName"].as_str().unwrap_or_default()),
                text(a["AttributeType"].as_str().unwrap_or_default()),
                text(""),
            ]);
        }
        for k in t["KeySchema"].as_array().map(Vec::as_slice).unwrap_or_default() {
            rows.push(vec![
                text("key"),
                text(k["AttributeName"].as_str().unwrap_or_default()),
                text(k["KeyType"].as_str().unwrap_or_default()),
                text(""),
            ]);
        }
        for (field, label) in [("GlobalSecondaryIndexes", "GSI"), ("LocalSecondaryIndexes", "LSI")]
        {
            for i in t[field].as_array().map(Vec::as_slice).unwrap_or_default() {
                let keys: Vec<&str> = i["KeySchema"]
                    .as_array()
                    .map(|k| k.iter().filter_map(|k| k["AttributeName"].as_str()).collect())
                    .unwrap_or_default();
                rows.push(vec![
                    text("index"),
                    text(i["IndexName"].as_str().unwrap_or_default()),
                    text(label),
                    text(&keys.join(",")),
                ]);
            }
        }
        Ok(ResultSet {
            columns: vec!["kind".into(), "name".into(), "type".into(), "detail".into()],
            rows,
            ..ResultSet::default()
        })
    }
}

/// One column per top-level attribute, in first-seen order; missing attributes are NULL.
fn items_to_result(items: &[Json], truncated: bool) -> ResultSet {
    let mut columns: Vec<String> = Vec::new();
    for item in items {
        for k in item.as_object().map(|o| o.keys()).into_iter().flatten() {
            if !columns.contains(k) {
                columns.push(k.clone());
            }
        }
    }
    let rows =
        items.iter().map(|i| columns.iter().map(|c| attribute(i.get(c))).collect()).collect();
    ResultSet { columns, rows, truncated, rows_affected: None }
}

/// DynamoDB wraps every value in a type tag: `{"S": "x"}`, `{"N": "1.5"}`, `{"M": {...}}`.
fn attribute(v: Option<&Json>) -> Value {
    let Some(obj) = v.and_then(Json::as_object) else { return Value::Null };
    let Some((tag, inner)) = obj.iter().next() else { return Value::Null };
    match (tag.as_str(), inner) {
        ("S", Json::String(s)) => Value::Text(s.clone()),
        ("N", Json::String(n)) => {
            n.parse::<i64>().map(Value::Int).unwrap_or_else(|_| Value::Text(n.clone()))
        }
        ("BOOL", Json::Bool(b)) => Value::Bool(*b),
        ("NULL", _) => Value::Null,
        ("B", Json::String(b)) => base64::engine::general_purpose::STANDARD
            .decode(b)
            .map_or_else(|_| Value::Text(b.clone()), Value::Bytes),
        _ => Value::Text(plain(v.unwrap_or(&Json::Null)).to_string()),
    }
}

/// Strips the type tags: `{"M":{"a":{"N":"1"}}}` becomes `{"a":1}`.
fn plain(v: &Json) -> Json {
    let Some((tag, inner)) = v.as_object().and_then(|o| o.iter().next()) else { return v.clone() };
    match (tag.as_str(), inner) {
        ("S", s) => s.clone(),
        ("N", Json::String(n)) => n
            .parse::<i64>()
            .map(Json::from)
            .or_else(|_| n.parse::<f64>().map(Json::from))
            .unwrap_or_else(|_| Json::String(n.clone())),
        ("BOOL", b) => b.clone(),
        ("NULL", _) => Json::Null,
        ("L", Json::Array(a)) => Json::Array(a.iter().map(plain).collect()),
        ("M", Json::Object(m)) => {
            Json::Object(m.iter().map(|(k, v)| (k.clone(), plain(v))).collect())
        }
        ("SS" | "BS", Json::Array(a)) => Json::Array(a.clone()),
        ("NS", Json::Array(a)) => {
            Json::Array(a.iter().map(|n| plain(&json!({ "N": n }))).collect())
        }
        _ => v.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(extra: &str) -> DataSourceConfig {
        toml::from_str(&format!("name='d'\ndialect='dynamo'\n{extra}")).unwrap()
    }

    #[test]
    fn target_uses_regional_endpoint_or_override() {
        let t = Target::from_config(&cfg("database='eu-west-1'")).unwrap();
        assert_eq!(
            (t.url.as_str(), t.host.as_str()),
            ("https://dynamodb.eu-west-1.amazonaws.com", "dynamodb.eu-west-1.amazonaws.com")
        );
        let t = Target::from_config(&cfg("database='local'\nhost='localhost'\nport=8000")).unwrap();
        assert_eq!((t.url.as_str(), t.host.as_str()), ("http://localhost:8000", "localhost:8000"));
        let t =
            Target::from_config(&cfg("database='x'\nhost='https://vpce.example.com/'")).unwrap();
        assert_eq!(
            (t.url.as_str(), t.host.as_str()),
            ("https://vpce.example.com", "vpce.example.com")
        );
    }

    #[test]
    fn attributes_lose_their_type_tags() {
        assert_eq!(attribute(Some(&json!({"S": "x"}))), Value::Text("x".into()));
        assert_eq!(attribute(Some(&json!({"N": "42"}))), Value::Int(42));
        assert_eq!(attribute(Some(&json!({"N": "1.50"}))), Value::Text("1.50".into()));
        assert_eq!(attribute(Some(&json!({"BOOL": true}))), Value::Bool(true));
        assert_eq!(attribute(Some(&json!({"NULL": true}))), Value::Null);
        assert_eq!(attribute(Some(&json!({"B": "AQI="}))), Value::Bytes(vec![1, 2]));
        assert_eq!(attribute(None), Value::Null);
        assert_eq!(
            attribute(Some(
                &json!({"M": {"a": {"N": "1"}, "l": {"L": [{"S": "z"}, {"BOOL": false}]}}})
            )),
            Value::Text(r#"{"a":1,"l":["z",false]}"#.into())
        );
        assert_eq!(attribute(Some(&json!({"NS": ["1", "2"]}))), Value::Text("[1,2]".into()));
    }

    #[test]
    fn items_become_rows() {
        let rs = items_to_result(
            &[
                json!({"id": {"S": "a"}, "n": {"N": "1"}}),
                json!({"id": {"S": "b"}, "x": {"S": "y"}}),
            ],
            false,
        );
        assert_eq!(rs.columns, ["id", "n", "x"]);
        assert_eq!(rs.rows[1], vec![Value::Text("b".into()), Value::Null, Value::Text("y".into())]);
    }

    #[test]
    fn api_errors_are_readable() {
        let e = api_error(
            400,
            r#"{"__type":"com.amazonaws.dynamodb.v20120810#ResourceNotFoundException","message":"Requested resource not found"}"#,
        );
        assert_eq!(e, "ResourceNotFoundException: Requested resource not found");
        assert!(api_error(502, "Bad Gateway").starts_with("HTTP 502"));
    }

    #[test]
    fn read_only_allows_select_only() {
        assert!(is_read_only("  SELECT * FROM t"));
        assert!(!is_read_only("DELETE FROM t WHERE a=1"));
        assert!(!is_read_only("sel"));
    }
}
