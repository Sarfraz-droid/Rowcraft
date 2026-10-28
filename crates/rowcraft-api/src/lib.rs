//! Local HTTP API over the saved data sources: every entry in `datasources.toml` (added with the
//! CLI or the desktop app) can be queried with `POST /datasources/{name}/query`.
//!
//! The data sources file is read on every request, so a connection added or edited while the
//! server runs is available at once. Each data source keeps one open [`Session`], so statements
//! on the same data source run one at a time, like in a console.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
use rowcraft_config::{DataSourceConfig, DataSourceFile};
use rowcraft_core::{Engine, Session};
use rowcraft_dialect::{IntrospectScope, ResultSet, Value};
use rowcraft_io::Format;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Mutex;

/// Rows returned when the request does not say; `max_rows` raises or lowers it.
pub const DEFAULT_MAX_ROWS: usize = 10_000;
/// Largest request body accepted (a SQL script).
const MAX_BODY: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Options {
    pub addr: SocketAddr,
    /// Required as `Authorization: Bearer <token>` when set.
    pub token: Option<String>,
    /// The `datasources.toml` to serve.
    pub datasources: PathBuf,
}

/// Starts listening on `options.addr` and serves until the process ends. Refuses to listen on a
/// non-loopback address without a token, since any request runs SQL with the saved credentials.
pub async fn serve(engine: Arc<Engine>, options: Options) -> std::io::Result<()> {
    if !options.addr.ip().is_loopback() && options.token.is_none() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("refusing to listen on {} without a token", options.addr),
        ));
    }
    let listener = tokio::net::TcpListener::bind(options.addr).await?;
    let api = Arc::new(Api::new(engine, options));
    loop {
        let (stream, _) = listener.accept().await?;
        let api = Arc::clone(&api);
        tokio::spawn(async move {
            let service = hyper::service::service_fn(move |req: Request<Incoming>| {
                let api = Arc::clone(&api);
                async move { Ok::<_, Infallible>(api.handle_incoming(req).await) }
            });
            // A client that hangs up mid-request is not worth reporting.
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(hyper_util::rt::TokioIo::new(stream), service)
                .await;
        });
    }
}

/// Request handling, separate from the socket so it can be tested directly.
#[derive(Debug)]
pub struct Api {
    engine: Arc<Engine>,
    options: Options,
    /// Open sessions by data source name, with the settings each was opened from.
    sessions: Mutex<HashMap<String, (DataSourceConfig, Arc<Session>)>>,
}

type Reply = Response<Full<Bytes>>;

/// Error carried to the client as `{"error": …, "kind": …}`.
#[derive(Debug)]
struct Failure {
    status: StatusCode,
    kind: &'static str,
    message: String,
}

impl Failure {
    fn new(status: StatusCode, kind: &'static str, message: impl std::fmt::Display) -> Self {
        Self { status, kind, message: message.to_string() }
    }

    fn bad_request(message: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "usage", message)
    }
}

impl From<rowcraft_core::Error> for Failure {
    fn from(e: rowcraft_core::Error) -> Self {
        use rowcraft_core::Error as C;
        use rowcraft_dialect::Error as D;
        let (status, kind) = match &e {
            C::Config(rowcraft_config::Error::NotFound(_)) => (StatusCode::NOT_FOUND, "not_found"),
            C::Config(_) => (StatusCode::BAD_REQUEST, "config"),
            C::Ssh(_) | C::Dialect(D::Connect(_)) => (StatusCode::BAD_GATEWAY, "connection"),
            C::Dialect(D::Query(_)) => (StatusCode::UNPROCESSABLE_ENTITY, "statement"),
            C::Dialect(D::NotImplemented(_) | D::UnknownDialect(_)) => {
                (StatusCode::NOT_IMPLEMENTED, "unsupported")
            }
            C::Cancelled => (StatusCode::CONFLICT, "cancelled"),
        };
        Self::new(status, kind, e)
    }
}

impl From<rowcraft_config::Error> for Failure {
    fn from(e: rowcraft_config::Error) -> Self {
        rowcraft_core::Error::from(e).into()
    }
}

/// `POST /datasources/{name}/query` body when sent as JSON; a plain-text body is the SQL itself.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryBody {
    #[serde(default)]
    sql: String,
    max_rows: Option<usize>,
    format: Option<String>,
}

impl Api {
    pub fn new(engine: Arc<Engine>, options: Options) -> Self {
        Self { engine, options, sessions: Mutex::new(HashMap::new()) }
    }

    async fn handle_incoming(&self, req: Request<Incoming>) -> Reply {
        let (parts, body) = req.into_parts();
        match Limited::new(body, MAX_BODY).collect().await {
            Ok(collected) => self.handle(Request::from_parts(parts, collected.to_bytes())).await,
            Err(_) => error_reply(&Failure::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "usage",
                format!("request body is over {MAX_BODY} bytes"),
            )),
        }
    }

    pub async fn handle(&self, req: Request<Bytes>) -> Reply {
        match self.route(req).await {
            Ok(reply) => reply,
            Err(failure) => error_reply(&failure),
        }
    }

    async fn route(&self, req: Request<Bytes>) -> Result<Reply, Failure> {
        self.authorize(req.headers())?;
        let path: Vec<String> =
            req.uri().path().split('/').filter(|s| !s.is_empty()).map(percent_decode).collect();
        let query = req.uri().query().unwrap_or("");
        let segments: Vec<&str> = path.iter().map(String::as_str).collect();
        match (req.method(), segments.as_slice()) {
            (&Method::GET, [] | ["health"]) => {
                Ok(json_reply(StatusCode::OK, &json!({ "status": "ok" })))
            }
            (&Method::GET, ["datasources"]) => self.list().await,
            (&Method::GET, ["datasources", name]) => self.describe(name).await,
            (&Method::POST, ["datasources", name, "query"]) => {
                self.query(name, query, req.headers(), req.body()).await
            }
            (&Method::POST, ["datasources", name, "test"]) => self.test(name).await,
            (&Method::POST, ["datasources", name, "cancel"]) => self.cancel(name).await,
            (&Method::GET, ["datasources", name, "catalog"]) => self.catalog(name, query).await,
            (&Method::DELETE, ["datasources", name, "session"]) => self.disconnect(name).await,
            (_, ["datasources", _, "query" | "test" | "cancel" | "catalog" | "session"])
            | (_, [] | ["health"] | ["datasources"] | ["datasources", _]) => Err(Failure::new(
                StatusCode::METHOD_NOT_ALLOWED,
                "usage",
                format!("{} is not allowed on {}", req.method(), req.uri().path()),
            )),
            _ => Err(Failure::new(
                StatusCode::NOT_FOUND,
                "not_found",
                format!("no endpoint {}; see GET /datasources", req.uri().path()),
            )),
        }
    }

    /// Browsers attach `Origin`; refusing those keeps web pages (including DNS-rebinding ones)
    /// from running SQL through a server meant for local tools like curl and Postman.
    fn authorize(&self, headers: &HeaderMap) -> Result<(), Failure> {
        if headers.contains_key(header::ORIGIN) {
            return Err(Failure::new(
                StatusCode::FORBIDDEN,
                "forbidden",
                "requests from web pages are refused",
            ));
        }
        if self.options.addr.ip().is_loopback() {
            let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("");
            if !is_loopback_host(host) {
                return Err(Failure::new(
                    StatusCode::FORBIDDEN,
                    "forbidden",
                    format!("unexpected Host `{host}`"),
                ));
            }
        }
        if let Some(token) = &self.options.token {
            let sent = headers
                .get(header::AUTHORIZATION)
                .and_then(|h| h.to_str().ok())
                .and_then(|h| h.strip_prefix("Bearer "))
                .unwrap_or("");
            if !constant_time_eq(sent.as_bytes(), token.as_bytes()) {
                return Err(Failure::new(
                    StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "missing or wrong `Authorization: Bearer` token",
                ));
            }
        }
        Ok(())
    }

    fn source(&self, name: &str) -> Result<DataSourceConfig, Failure> {
        let file = DataSourceFile::load(&self.options.datasources)?;
        Ok(file.get(name)?.clone())
    }

    async fn summary(&self, ds: &DataSourceConfig) -> serde_json::Value {
        let connected = self.sessions.lock().await.contains_key(&ds.name);
        let base = format!("/datasources/{}", percent_encode(&ds.name));
        json!({
            "name": ds.name,
            "dialect": ds.dialect,
            "driver": ds.driver,
            "host": ds.host,
            "port": ds.port,
            "database": ds.database,
            "user": ds.user,
            "read_only": ds.read_only,
            "comment": ds.comment,
            "connected": connected,
            "endpoints": {
                "query": format!("POST {base}/query"),
                "catalog": format!("GET {base}/catalog"),
                "test": format!("POST {base}/test"),
                "cancel": format!("POST {base}/cancel"),
                "disconnect": format!("DELETE {base}/session"),
            },
        })
    }

    async fn list(&self) -> Result<Reply, Failure> {
        let file = DataSourceFile::load(&self.options.datasources)?;
        let mut out = Vec::with_capacity(file.datasources.len());
        for ds in &file.datasources {
            out.push(self.summary(ds).await);
        }
        Ok(json_reply(StatusCode::OK, &serde_json::Value::Array(out)))
    }

    async fn describe(&self, name: &str) -> Result<Reply, Failure> {
        let ds = self.source(name)?;
        Ok(json_reply(StatusCode::OK, &self.summary(&ds).await))
    }

    /// The open session for `name`, opened (or reopened when its settings changed) as needed.
    async fn session(&self, name: &str) -> Result<Arc<Session>, Failure> {
        let ds = self.source(name)?;
        if let Some((cfg, session)) = self.sessions.lock().await.get(name) {
            if *cfg == ds {
                return Ok(Arc::clone(session));
            }
        }
        let session = self.engine.open_session(&ds).await?;
        self.sessions.lock().await.insert(name.to_owned(), (ds, Arc::clone(&session)));
        Ok(session)
    }

    async fn query(
        &self,
        name: &str,
        query: &str,
        headers: &HeaderMap,
        body: &Bytes,
    ) -> Result<Reply, Failure> {
        let params = parse_query(query);
        let is_json = headers
            .get(header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .is_some_and(|t| t.trim_start().starts_with("application/json"));
        let body: QueryBody = if is_json {
            serde_json::from_slice(body)
                .map_err(|e| Failure::bad_request(format!("invalid JSON body: {e}")))?
        } else {
            let sql = std::str::from_utf8(body)
                .map_err(|_| Failure::bad_request("the SQL body is not UTF-8"))?;
            QueryBody { sql: sql.to_owned(), ..QueryBody::default() }
        };
        // `?sql=` is accepted for quick tests, but only on POST, never on a GET a page could embed.
        let sql = match param(&params, "sql") {
            Some(sql) if body.sql.trim().is_empty() => sql.to_owned(),
            _ => body.sql,
        };
        if sql.trim().is_empty() {
            return Err(Failure::bad_request(
                "no SQL: send it as the body, as {\"sql\": …}, or as ?sql=",
            ));
        }
        let max_rows = match param(&params, "max_rows") {
            Some(n) => n
                .parse()
                .map_err(|_| Failure::bad_request(format!("max_rows `{n}` is not a number")))?,
            None => body.max_rows.unwrap_or(DEFAULT_MAX_ROWS),
        };
        let format = param(&params, "format").map(str::to_owned).or(body.format);
        let format = match format.as_deref() {
            None | Some("json") => None,
            Some(other) => Some(other.parse::<Format>().map_err(Failure::bad_request)?),
        };

        let session = self.session(name).await?;
        let started = Instant::now();
        let result = match session.query(&sql, max_rows).await {
            // The server closed an idle connection; the statement never ran, so run it again.
            Err(e) if rowcraft_core::is_connection_lost(&e) => {
                session.reconnect(&self.engine).await?;
                session.query(&sql, max_rows).await?
            }
            other => other?,
        };
        let elapsed_ms = started.elapsed().as_millis();
        match format {
            None => Ok(json_reply(StatusCode::OK, &result_json(&result, elapsed_ms))),
            Some(format) => {
                let mut out = Vec::new();
                rowcraft_io::write_result(&result, format, &mut out)
                    .map_err(|e| Failure::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", e))?;
                Ok(reply(StatusCode::OK, content_type(format), out))
            }
        }
    }

    /// Opens a fresh connection and closes it, leaving any open session alone.
    async fn test(&self, name: &str) -> Result<Reply, Failure> {
        let ds = self.source(name)?;
        let started = Instant::now();
        drop(self.engine.connect(&ds).await?);
        Ok(json_reply(
            StatusCode::OK,
            &json!({ "ok": true, "elapsed_ms": started.elapsed().as_millis() }),
        ))
    }

    async fn cancel(&self, name: &str) -> Result<Reply, Failure> {
        self.source(name)?;
        let session = self.sessions.lock().await.get(name).map(|(_, s)| Arc::clone(s));
        let cancelled = match session {
            Some(session) => session.cancel().await,
            None => false,
        };
        Ok(json_reply(StatusCode::OK, &json!({ "cancelled": cancelled })))
    }

    async fn catalog(&self, name: &str, query: &str) -> Result<Reply, Failure> {
        let schemas: Vec<String> = parse_query(query)
            .into_iter()
            .filter(|(k, v)| k == "schema" && !v.is_empty())
            .map(|(_, v)| v)
            .collect();
        let scope = IntrospectScope { schemas: (!schemas.is_empty()).then_some(schemas) };
        let session = self.session(name).await?;
        let catalog = match session.introspect(&scope).await {
            Err(e) if rowcraft_core::is_connection_lost(&e) => {
                session.reconnect(&self.engine).await?;
                session.introspect(&scope).await?
            }
            other => other?,
        };
        let value = serde_json::to_value(&catalog)
            .map_err(|e| Failure::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", e))?;
        Ok(json_reply(StatusCode::OK, &value))
    }

    async fn disconnect(&self, name: &str) -> Result<Reply, Failure> {
        let closed = self.sessions.lock().await.remove(name).is_some();
        Ok(json_reply(StatusCode::OK, &json!({ "closed": closed })))
    }
}

fn result_json(rs: &ResultSet, elapsed_ms: u128) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = rs
        .rows
        .iter()
        .map(|row| serde_json::Value::Array(row.iter().map(value_json).collect()))
        .collect();
    json!({
        "columns": rs.columns,
        "rows": rows,
        "row_count": rs.rows.len(),
        "rows_affected": rs.rows_affected,
        "truncated": rs.truncated,
        "elapsed_ms": elapsed_ms,
    })
}

fn value_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => json!(b),
        Value::Int(i) => json!(i),
        // NaN and infinities have no JSON form.
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map_or_else(|| json!(f.to_string()), serde_json::Value::Number),
        Value::Text(s) => json!(s),
        Value::Bytes(b) => json!(b.iter().map(|byte| format!("{byte:02x}")).collect::<String>()),
    }
}

fn content_type(format: Format) -> &'static str {
    match format {
        Format::Csv => "text/csv; charset=utf-8",
        Format::Tsv => "text/tab-separated-values; charset=utf-8",
        Format::Json => "application/json",
        Format::Ndjson => "application/x-ndjson",
        Format::Markdown => "text/markdown; charset=utf-8",
        Format::Table => "text/plain; charset=utf-8",
    }
}

fn reply(status: StatusCode, content_type: &'static str, body: Vec<u8>) -> Reply {
    let mut response = Response::new(Full::new(Bytes::from(body)));
    *response.status_mut() = status;
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
}

fn json_reply(status: StatusCode, value: &serde_json::Value) -> Reply {
    let mut body = serde_json::to_vec_pretty(value).unwrap_or_default();
    body.push(b'\n');
    reply(status, "application/json", body)
}

fn error_reply(failure: &Failure) -> Reply {
    json_reply(failure.status, &json!({ "error": failure.message, "kind": failure.kind }))
}

fn is_loopback_host(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(""),
        None => host.rsplit_once(':').map_or(host, |(name, _)| name),
    };
    name.eq_ignore_ascii_case("localhost")
        || name.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(&k.replace('+', " ")), percent_decode(&v.replace('+', " ")))
        })
        .collect()
}

fn param<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
    params.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn percent_decode(s: &str) -> String {
    let hex = |b: Option<&u8>| b.and_then(|b| (*b as char).to_digit(16));
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], hex(bytes.get(i + 1)), hex(bytes.get(i + 2))) {
            (b'%', Some(hi), Some(lo)) => {
                out.push(u8::try_from(hi * 16 + lo).unwrap_or(b'?'));
                i += 3;
            }
            (b, ..) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests;
