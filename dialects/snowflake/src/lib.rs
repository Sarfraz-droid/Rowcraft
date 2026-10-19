//! Snowflake dialect (tier B, see SPEC.md §5) over the REST API Snowflake's own drivers use
//! (`/session/v1/login-request`, `/queries/v1/query-request`), with `reqwest` and rustls.
//!
//! * Login is user + password (the password comes from the data source's password variable).
//!   Key-pair (JWT), SSO and MFA are not implemented.
//! * The server is asked for JSON results (`JDBC_QUERY_RESULT_FORMAT`), not Arrow: small results
//!   are inline in `rowset`, larger ones are fetched chunk by chunk and stop at the row limit.
//! * `host` is the account (`xy12345.eu-west-1`) or a full `….snowflakecomputing.com` host.
//!   `database` and these Advanced properties are used when set: `warehouse`, `role`, `schema`.
//! * Every value arrives as text, like the pg dialect.

mod introspect;

use async_trait::async_trait;
use rowcraft_config::DataSourceConfig;
use rowcraft_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result,
    ResultSet, Value, split_statements,
};
use rowcraft_model::Catalog;
use serde_json::{Value as Json, json};

#[derive(Debug, Default, Clone, Copy)]
pub struct SnowflakeDialect;

#[async_trait]
impl Dialect for SnowflakeDialect {
    fn id(&self) -> DialectId {
        DialectId("snowflake")
    }

    fn display_name(&self) -> &'static str {
        "Snowflake"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn default_port(&self) -> Option<u16> {
        Some(443)
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
            .user_agent("Rowcraft/0.0.1")
            .gzip(true)
            .build()
            .map_err(|e| Error::Connect(e.to_string()))?;
        let endpoint = Endpoint::from_config(cfg)?;
        let token = login(&client, &endpoint, cfg).await?;
        Ok(Box::new(SnowflakeConnection {
            client,
            endpoint,
            token,
            sequence: 0,
            read_only: cfg.read_only,
        }))
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
struct Endpoint {
    /// `https://host[:port]`, no trailing slash.
    base: String,
    /// Account locator, the first label of the host.
    account: String,
}

impl Endpoint {
    fn from_config(cfg: &DataSourceConfig) -> Result<Self> {
        let raw =
            cfg.host.as_deref().map(str::trim).filter(|h| !h.is_empty()).ok_or_else(|| {
                Error::Connect("Snowflake needs an account in Host (e.g. xy12345.eu-west-1)".into())
            })?;
        let raw = raw.trim_start_matches("https://").trim_end_matches('/');
        let host = if raw.ends_with(".snowflakecomputing.com")
            || raw.ends_with(".snowflakecomputing.cn")
        {
            raw.to_owned()
        } else {
            format!("{raw}.snowflakecomputing.com")
        };
        let account = host.split('.').next().unwrap_or_default().to_owned();
        let base = match cfg.port {
            None | Some(443) => format!("https://{host}"),
            Some(p) => format!("https://{host}:{p}"),
        };
        Ok(Self { base, account })
    }
}

fn property(cfg: &DataSourceConfig, key: &str) -> Option<String> {
    cfg.properties.get(key).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

async fn login(client: &reqwest::Client, ep: &Endpoint, cfg: &DataSourceConfig) -> Result<String> {
    let mut query: Vec<(&str, String)> = vec![("requestId", request_id(0))];
    for (param, value) in [
        ("databaseName", cfg.database.clone().filter(|d| !d.is_empty())),
        ("schemaName", property(cfg, "schema")),
        ("warehouse", property(cfg, "warehouse")),
        ("roleName", property(cfg, "role")),
    ] {
        if let Some(v) = value {
            query.push((param, v));
        }
    }
    let body = json!({ "data": {
        "ACCOUNT_NAME": ep.account,
        "LOGIN_NAME": cfg.user.clone().unwrap_or_default(),
        "PASSWORD": cfg.password.clone().unwrap_or_default(),
        "CLIENT_APP_ID": "Rowcraft",
        "CLIENT_APP_VERSION": "0.0.1",
        "SVN_REVISION": "0",
        "CLIENT_ENVIRONMENT": { "APPLICATION": "Rowcraft" },
        "SESSION_PARAMETERS": { "JDBC_QUERY_RESULT_FORMAT": "JSON", "CLIENT_SESSION_KEEP_ALIVE": false },
    }});
    let response = client
        .post(format!("{}/session/v1/login-request", ep.base))
        .query(&query)
        .header("Accept", "application/snowflake")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Connect(e.to_string()))?;
    let status = response.status().as_u16();
    let text = response.text().await.map_err(|e| Error::Connect(e.to_string()))?;
    parse_login(status, &text)
}

fn parse_login(status: u16, body: &str) -> Result<String> {
    let json: Json = serde_json::from_str(body).map_err(|_| {
        Error::Connect(format!(
            "HTTP {status}: {}",
            body.trim().chars().take(200).collect::<String>()
        ))
    })?;
    if json["success"].as_bool() != Some(true) {
        return Err(Error::Connect(json["message"].as_str().unwrap_or("login failed").to_owned()));
    }
    json["data"]["token"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::Connect("login response has no token".into()))
}

/// Unique enough for the server's idempotency key; not a security token.
fn request_id(n: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let a = (t as u64) ^ n.rotate_left(32);
    let b = (t >> 64) as u64 ^ 0x9e37_79b9_7f4a_7c15 ^ n;
    format!(
        "{:08x}-{:04x}-4{:03x}-a{:03x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xffff,
        a & 0xfff,
        (b >> 48) & 0xfff,
        b & 0xffff_ffff_ffff
    )
}

#[derive(Debug)]
struct SnowflakeConnection {
    client: reqwest::Client,
    endpoint: Endpoint,
    token: String,
    sequence: u64,
    read_only: bool,
}

#[async_trait]
impl Connection for SnowflakeConnection {
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

/// Leading keyword check; Snowflake has no read-only session mode.
fn is_read_only(stmt: &str) -> bool {
    let word = stmt.trim_start().split(|c: char| !c.is_ascii_alphabetic()).next().unwrap_or("");
    matches!(
        word.to_ascii_uppercase().as_str(),
        "SELECT" | "WITH" | "SHOW" | "DESCRIBE" | "DESC" | "EXPLAIN" | "USE" | "LIST" | "VALUES"
    )
}

impl SnowflakeConnection {
    fn auth(&self) -> String {
        format!("Snowflake Token=\"{}\"", self.token)
    }

    async fn run(&mut self, stmt: &str, max_rows: usize) -> Result<ResultSet> {
        self.sequence += 1;
        let submitted = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let mut reply = self
            .post(
                "/queries/v1/query-request",
                &[("requestId", request_id(self.sequence))],
                &json!({ "sqlText": stmt, "asyncExec": false, "sequenceId": self.sequence, "querySubmissionTime": submitted }),
            )
            .await?;
        // 333333/333334: still running; the reply says where to poll.
        let mut polls = 0;
        while matches!(code(&reply), Some(333_333 | 333_334)) {
            polls += 1;
            if polls > 3600 {
                return Err(Error::Query("query is still running after an hour".into()));
            }
            tokio::time::sleep(std::time::Duration::from_millis(if polls < 10 {
                200
            } else {
                1000
            }))
            .await;
            let url = reply["data"]["getResultUrl"]
                .as_str()
                .ok_or_else(|| Error::Query("query in progress but no result URL".into()))?
                .to_owned();
            reply = self.get_json(&url).await?;
        }
        let mut rs = parse_result(&reply, max_rows)?;
        // Larger results continue in chunks; stop as soon as the limit is reached.
        if let Some(chunks) = reply["data"]["chunks"].as_array() {
            let mut headers: Vec<(String, String)> = reply["data"]["chunkHeaders"]
                .as_object()
                .map(|h| {
                    h.iter()
                        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                        .collect()
                })
                .unwrap_or_default();
            if headers.is_empty() {
                if let Some(key) = reply["data"]["qrmk"].as_str() {
                    headers.push((
                        "x-amz-server-side-encryption-customer-algorithm".into(),
                        "AES256".into(),
                    ));
                    headers
                        .push(("x-amz-server-side-encryption-customer-key".into(), key.to_owned()));
                }
            }
            for chunk in chunks {
                let Some(url) = chunk["url"].as_str() else { continue };
                if rs.rows.len() >= max_rows {
                    rs.truncated = true;
                    break;
                }
                let mut req = self.client.get(url);
                for (k, v) in &headers {
                    req = req.header(k, v);
                }
                let body = req
                    .send()
                    .await
                    .and_then(reqwest::Response::error_for_status)
                    .map_err(|e| Error::Query(e.to_string()))?
                    .text()
                    .await
                    .map_err(|e| Error::Query(e.to_string()))?;
                // A chunk is a JSON array's elements without the surrounding brackets.
                let rows: Vec<Vec<Json>> = serde_json::from_str(&format!("[{body}]"))
                    .map_err(|e| Error::Query(format!("bad result chunk: {e}")))?;
                for row in rows {
                    if rs.rows.len() >= max_rows {
                        rs.truncated = true;
                        break;
                    }
                    rs.rows.push(row.iter().map(cell).collect());
                }
            }
        }
        Ok(rs)
    }

    async fn post(&self, path: &str, query: &[(&str, String)], body: &Json) -> Result<Json> {
        let response = self
            .client
            .post(format!("{}{path}", self.endpoint.base))
            .query(query)
            .header("Authorization", self.auth())
            .header("Accept", "application/snowflake")
            .json(body)
            .send()
            .await
            .map_err(|e| Error::Query(e.to_string()))?;
        decode(response).await
    }

    async fn get_json(&self, path: &str) -> Result<Json> {
        let response = self
            .client
            .get(format!("{}{path}", self.endpoint.base))
            .header("Authorization", self.auth())
            .header("Accept", "application/snowflake")
            .send()
            .await
            .map_err(|e| Error::Query(e.to_string()))?;
        decode(response).await
    }
}

async fn decode(response: reqwest::Response) -> Result<Json> {
    let status = response.status().as_u16();
    let text = response.text().await.map_err(|e| Error::Query(e.to_string()))?;
    serde_json::from_str(&text).map_err(|_| {
        Error::Query(format!(
            "HTTP {status}: {}",
            text.trim().chars().take(200).collect::<String>()
        ))
    })
}

fn code(reply: &Json) -> Option<i64> {
    match &reply["code"] {
        Json::String(s) => s.parse().ok(),
        Json::Number(n) => n.as_i64(),
        _ => None,
    }
}

/// Builds the first page of a result from a query reply.
fn parse_result(reply: &Json, max_rows: usize) -> Result<ResultSet> {
    if reply["success"].as_bool() != Some(true) {
        let message = reply["message"].as_str().unwrap_or("query failed");
        // Codes are zero-padded strings (`002003`); keep them as the server wrote them.
        return Err(Error::Query(match &reply["code"] {
            Json::String(c) => format!("{c}: {message}"),
            Json::Number(c) => format!("{c}: {message}"),
            _ => message.to_owned(),
        }));
    }
    let data = &reply["data"];
    let columns: Vec<String> = data["rowtype"]
        .as_array()
        .map(|t| t.iter().map(|c| c["name"].as_str().unwrap_or_default().to_owned()).collect())
        .unwrap_or_default();
    let rowset = data["rowset"].as_array().map(Vec::as_slice).unwrap_or_default();
    let mut rs = ResultSet { columns, ..ResultSet::default() };
    for row in rowset {
        if rs.rows.len() >= max_rows {
            rs.truncated = true;
            break;
        }
        rs.rows.push(row.as_array().map(|r| r.iter().map(cell).collect()).unwrap_or_default());
    }
    Ok(rs)
}

/// The JSON format sends every value as a string (or null).
fn cell(v: &Json) -> Value {
    match v {
        Json::Null => Value::Null,
        Json::String(s) => Value::Text(s.clone()),
        other => Value::Text(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(extra: &str) -> DataSourceConfig {
        toml::from_str(&format!("name='s'\ndialect='snowflake'\n{extra}")).unwrap()
    }

    #[test]
    fn account_becomes_host() {
        let e = Endpoint::from_config(&cfg("host='xy12345.eu-west-1'")).unwrap();
        assert_eq!(e.base, "https://xy12345.eu-west-1.snowflakecomputing.com");
        assert_eq!(e.account, "xy12345");
        let e =
            Endpoint::from_config(&cfg("host='https://org-acct.snowflakecomputing.com/'")).unwrap();
        assert_eq!(e.base, "https://org-acct.snowflakecomputing.com");
        assert_eq!(e.account, "org-acct");
        assert!(Endpoint::from_config(&cfg("")).is_err());
    }

    #[test]
    fn login_replies() {
        assert_eq!(parse_login(200, r#"{"success":true,"data":{"token":"T"}}"#).unwrap(), "T");
        let e = parse_login(
            200,
            r#"{"success":false,"message":"Incorrect username or password was specified."}"#,
        )
        .unwrap_err();
        assert!(e.to_string().contains("Incorrect username"));
        assert!(parse_login(502, "Bad Gateway").unwrap_err().to_string().contains("HTTP 502"));
    }

    #[test]
    fn parses_inline_rowset_and_errors() {
        let ok = json!({"success":true,"data":{"rowtype":[{"name":"A"},{"name":"B"}],"rowset":[["1","x"],["2",null],["3","z"]]}});
        let rs = parse_result(&ok, 2).unwrap();
        assert_eq!(rs.columns, ["A", "B"]);
        assert_eq!(rs.rows[1], vec![Value::Text("2".into()), Value::Null]);
        assert!(rs.truncated);
        let bad = json!({"success":false,"code":"002003","message":"SQL compilation error: Object does not exist"});
        assert!(
            parse_result(&bad, 10)
                .unwrap_err()
                .to_string()
                .contains("002003: SQL compilation error")
        );
        assert_eq!(code(&json!({"code":"333334"})), Some(333_334));
    }

    #[test]
    fn read_only_screen_and_request_ids() {
        assert!(is_read_only("  select 1"));
        assert!(is_read_only("SHOW TABLES"));
        assert!(!is_read_only("drop table t"));
        assert!(!is_read_only("insert into t values (1)"));
        let (a, b) = (request_id(1), request_id(2));
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
    }
}
