//! Model Context Protocol server over stdio, so an AI client (Claude Code, Codex, Cursor, …) can
//! list, inspect and query the saved data sources.
//!
//! The transport is newline-delimited JSON-RPC 2.0 on stdin/stdout; nothing but protocol
//! messages may be written to stdout. The data sources file is read on every call, so a
//! connection added while the client is running is available at once.
//!
//! Statements are read-only unless the server is started with `allow_write`. The check in
//! [`check_read_only`] is a guard rail against an assistant making a mistake, not a security
//! boundary: mark the data source `read_only` (or use a read-only database user) to enforce it.

mod guard;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use rowcraft_config::{DataSourceConfig, DataSourceFile};
use rowcraft_core::{Engine, Session};
use rowcraft_dialect::IntrospectScope;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

pub use guard::check_read_only;

/// Protocol revisions this server can speak, newest first.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// Rows returned when the call does not say. Small on purpose: results go into the model's context.
pub const DEFAULT_MAX_ROWS: usize = 200;
/// Largest `max_rows` a call may ask for.
pub const MAX_ROWS_LIMIT: usize = 5_000;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

#[derive(Debug, Clone)]
pub struct Options {
    /// The `datasources.toml` to serve.
    pub datasources: PathBuf,
    /// Allow statements that change data or schema.
    pub allow_write: bool,
}

#[derive(Debug)]
pub struct Server {
    engine: Arc<Engine>,
    options: Options,
    /// Open sessions by data source name, with the settings each was opened from.
    sessions: Mutex<HashMap<String, (DataSourceConfig, Arc<Session>)>>,
}

/// Serves MCP on stdin/stdout until the client closes the pipe.
pub async fn serve_stdio(engine: Arc<Engine>, options: Options) -> std::io::Result<()> {
    let server = Arc::new(Server::new(engine, options));
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    // One writer owns stdout so replies from concurrent calls never interleave.
    let writer = tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(line) = rx.recv().await {
            if out.write_all(line.as_bytes()).await.is_err()
                || out.write_all(b"\n").await.is_err()
                || out.flush().await.is_err()
            {
                break;
            }
        }
    });
    let mut calls = tokio::task::JoinSet::new();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let (server, tx) = (Arc::clone(&server), tx.clone());
        // Calls run concurrently so a slow query does not block `ping`.
        calls.spawn(async move {
            if let Some(reply) = server.handle_line(&line).await {
                let _ = tx.send(reply.to_string());
            }
        });
    }
    // The client closed stdin: finish what is running, then flush the replies.
    while calls.join_next().await.is_some() {}
    drop(tx);
    let _ = writer.await;
    Ok(())
}

impl Server {
    pub fn new(engine: Arc<Engine>, options: Options) -> Self {
        Self { engine, options, sessions: Mutex::new(HashMap::new()) }
    }

    /// Handles one line of input; `None` for notifications, which get no reply.
    pub async fn handle_line(&self, line: &str) -> Option<Value> {
        match serde_json::from_str::<Value>(line) {
            Ok(Value::Array(_)) => {
                Some(rpc_error(Value::Null, INVALID_REQUEST, "batch requests are not supported"))
            }
            Ok(message) => self.handle(message).await,
            Err(e) => Some(rpc_error(Value::Null, PARSE_ERROR, format!("invalid JSON: {e}"))),
        }
    }

    pub async fn handle(&self, message: Value) -> Option<Value> {
        let id = message.get("id").cloned();
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A response to a request we never send, or garbage.
            return id.map(|id| rpc_error(id, INVALID_REQUEST, "missing `method`"));
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        // Notifications (`notifications/initialized`, `notifications/cancelled`, …) get no reply.
        let id = id?;
        let outcome = match method {
            "initialize" => Ok(initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => self.call_tool(&params).await,
            "resources/list" => Ok(json!({ "resources": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            other => Err((METHOD_NOT_FOUND, format!("unknown method `{other}`"))),
        };
        Some(match outcome {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => rpc_error(id, code, message),
        })
    }

    async fn call_tool(&self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((INVALID_PARAMS, "missing tool `name`".to_owned()))?;
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let outcome = match name {
            "list_datasources" => self.list_datasources(),
            "get_catalog" => self.get_catalog(&args).await,
            "run_query" => self.run_query(&args).await,
            "test_connection" => self.test_connection(&args).await,
            other => return Err((INVALID_PARAMS, format!("unknown tool `{other}`"))),
        };
        // Failures the model can act on (bad SQL, unknown source) are tool results, not
        // protocol errors, so the assistant sees the message and can correct itself.
        Ok(match outcome {
            Ok(value) => tool_result(&value, false),
            Err(message) => tool_result(&Value::String(message), true),
        })
    }

    fn load(&self) -> Result<DataSourceFile, String> {
        DataSourceFile::load(&self.options.datasources).map_err(|e| e.to_string())
    }

    fn source(&self, args: &Value) -> Result<DataSourceConfig, String> {
        let name = str_arg(args, "datasource")?;
        let file = self.load()?;
        file.get(name).cloned().map_err(|e| {
            let known: Vec<&str> = file.datasources.iter().map(|d| d.name.as_str()).collect();
            format!("{e}. Saved data sources: [{}]", known.join(", "))
        })
    }

    fn list_datasources(&self) -> Result<Value, String> {
        let file = self.load()?;
        let sources: Vec<Value> = file
            .datasources
            .iter()
            .map(|ds| {
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
                })
            })
            .collect();
        Ok(json!({ "datasources": sources, "writes_allowed": self.options.allow_write }))
    }

    /// The open session for `ds`, opened (or reopened when its settings changed) as needed.
    async fn session(&self, ds: &DataSourceConfig) -> Result<Arc<Session>, String> {
        if let Some((cfg, session)) = self.sessions.lock().await.get(&ds.name) {
            if cfg == ds {
                return Ok(Arc::clone(session));
            }
        }
        let session = self.engine.open_session(ds).await.map_err(|e| e.to_string())?;
        self.sessions.lock().await.insert(ds.name.clone(), (ds.clone(), Arc::clone(&session)));
        Ok(session)
    }

    async fn run_query(&self, args: &Value) -> Result<Value, String> {
        let ds = self.source(args)?;
        let sql = str_arg(args, "sql")?;
        if sql.trim().is_empty() {
            return Err("`sql` is empty".to_owned());
        }
        if !self.options.allow_write {
            check_read_only(sql)?;
        }
        let max_rows = match args.get("max_rows") {
            None | Some(Value::Null) => DEFAULT_MAX_ROWS,
            Some(n) => n
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or("`max_rows` must be a positive integer")?
                .min(MAX_ROWS_LIMIT),
        };
        let session = self.session(&ds).await?;
        let started = Instant::now();
        let result = match session.query(sql, max_rows).await {
            // The server closed an idle connection; the statement never ran, so run it again.
            Err(e) if rowcraft_core::is_connection_lost(&e) => {
                session.reconnect(&self.engine).await.map_err(|e| e.to_string())?;
                session.query(sql, max_rows).await.map_err(|e| e.to_string())?
            }
            other => other.map_err(|e| e.to_string())?,
        };
        Ok(rowcraft_api::result_json(&result, started.elapsed().as_millis()))
    }

    async fn get_catalog(&self, args: &Value) -> Result<Value, String> {
        let ds = self.source(args)?;
        let schemas: Vec<String> = match args.get("schemas") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(items)) => items
                .iter()
                .map(|v| v.as_str().map(str::to_owned).ok_or("`schemas` must be strings"))
                .collect::<Result<_, _>>()?,
            Some(_) => return Err("`schemas` must be an array of strings".to_owned()),
        };
        let scope = IntrospectScope { schemas: (!schemas.is_empty()).then_some(schemas) };
        let session = self.session(&ds).await?;
        let catalog = match session.introspect(&scope).await {
            Err(e) if rowcraft_core::is_connection_lost(&e) => {
                session.reconnect(&self.engine).await.map_err(|e| e.to_string())?;
                session.introspect(&scope).await.map_err(|e| e.to_string())?
            }
            other => other.map_err(|e| e.to_string())?,
        };
        serde_json::to_value(&catalog).map_err(|e| format!("cannot encode catalog: {e}"))
    }

    /// Opens a fresh connection and closes it, leaving any open session alone.
    async fn test_connection(&self, args: &Value) -> Result<Value, String> {
        let ds = self.source(args)?;
        let started = Instant::now();
        drop(self.engine.connect(&ds).await.map_err(|e| e.to_string())?);
        Ok(json!({ "ok": true, "elapsed_ms": started.elapsed().as_millis() }))
    }
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing required string argument `{key}`"))
}

fn rpc_error(id: Value, code: i64, message: impl std::fmt::Display) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.to_string() } })
}

fn tool_result(value: &Value, is_error: bool) -> Value {
    let text = match value {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version =
        PROTOCOL_VERSIONS.iter().find(|v| Some(**v) == requested).unwrap_or(&PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "rowcraft", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Query the databases saved in Rowcraft. Start with list_datasources, \
            then get_catalog to learn table and column names before writing SQL with run_query. \
            Statements are read-only unless the server was started with --allow-write.",
    })
}

fn tools() -> Value {
    let source =
        json!({ "type": "string", "description": "Data source name from list_datasources." });
    json!([
        {
            "name": "list_datasources",
            "description": "List the saved Rowcraft data sources (name, database type, host, \
                database, read-only flag). Credentials are never included.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "title": "List data sources", "readOnlyHint": true },
        },
        {
            "name": "get_catalog",
            "description": "Read a data source's catalog: schemas, tables, columns, indexes, keys \
                and routines. Use it to learn the real names before writing SQL.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "datasource": source,
                    "schemas": {
                        "type": "array", "items": { "type": "string" },
                        "description": "Only these schemas. Default: every user schema. Pass it for large databases."
                    }
                },
                "required": ["datasource"],
                "additionalProperties": false
            },
            "annotations": { "title": "Read catalog", "readOnlyHint": true },
        },
        {
            "name": "run_query",
            "description": "Run one statement against a data source and return columns and rows. \
                Read-only unless the server allows writes. Add a LIMIT and ask only for the \
                columns you need.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "datasource": source,
                    "sql": { "type": "string", "description": "A single statement in the data source's own dialect." },
                    "max_rows": {
                        "type": "integer", "minimum": 1, "maximum": MAX_ROWS_LIMIT,
                        "description": format!("Rows to return before truncating. Default {DEFAULT_MAX_ROWS}.")
                    }
                },
                "required": ["datasource", "sql"],
                "additionalProperties": false
            },
            "annotations": { "title": "Run query", "readOnlyHint": false, "destructiveHint": true },
        },
        {
            "name": "test_connection",
            "description": "Connect to a data source and report success or the connection error.",
            "inputSchema": {
                "type": "object",
                "properties": { "datasource": source },
                "required": ["datasource"],
                "additionalProperties": false
            },
            "annotations": { "title": "Test connection", "readOnlyHint": true },
        },
    ])
}

#[cfg(test)]
mod tests;
