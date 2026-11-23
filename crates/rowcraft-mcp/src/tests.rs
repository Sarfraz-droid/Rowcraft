#![allow(clippy::unwrap_used)]

use super::*;
use rowcraft_dialect::Registry;

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A server over a data sources file with one SQLite source (`local db`) in a fresh directory.
fn server(allow_write: bool) -> (Server, TempDir) {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rowcraft-mcp-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let datasources = dir.join("datasources.toml");
    DataSourceFile {
        datasources: vec![DataSourceConfig {
            name: "local db".into(),
            dialect: "sqlite".into(),
            database: Some(dir.join("t.db").display().to_string()),
            ..Default::default()
        }],
    }
    .save(&datasources)
    .unwrap();
    let mut registry = Registry::new();
    registry.register(rowcraft_dialect_sqlite::SqliteDialect);
    let server = Server::new(Arc::new(Engine::new(registry)), Options { datasources, allow_write });
    (server, TempDir(dir))
}

async fn rpc(server: &Server, message: Value) -> Value {
    server.handle(message).await.expect("a reply")
}

async fn call(server: &Server, tool: &str, arguments: Value) -> (bool, Value) {
    let reply = rpc(
        server,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": tool, "arguments": arguments } }),
    )
    .await;
    let result = &reply["result"];
    let text = result["content"][0]["text"].as_str().unwrap();
    let value = serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned()));
    (result["isError"].as_bool().unwrap(), value)
}

#[tokio::test]
async fn initialize_negotiates_a_known_version() {
    let (server, _dir) = server(false);
    let reply = rpc(
        &server,
        json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize",
                "params": { "protocolVersion": "2024-11-05" } }),
    )
    .await;
    assert_eq!(reply["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(reply["result"]["serverInfo"]["name"], "rowcraft");
    assert!(reply["result"]["capabilities"]["tools"].is_object());

    let unknown = rpc(
        &server,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "1999-01-01" } }),
    )
    .await;
    assert_eq!(unknown["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
}

#[tokio::test]
async fn notifications_get_no_reply() {
    let (server, _dir) = server(false);
    let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert!(server.handle(note).await.is_none());
}

#[tokio::test]
async fn bad_input_is_a_protocol_error() {
    let (server, _dir) = server(false);
    let parse = server.handle_line("{nope").await.unwrap();
    assert_eq!(parse["error"]["code"], PARSE_ERROR);
    let unknown =
        rpc(&server, json!({ "jsonrpc": "2.0", "id": 2, "method": "nope/nothing" })).await;
    assert_eq!(unknown["error"]["code"], METHOD_NOT_FOUND);
    let tool = rpc(
        &server,
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "nope" } }),
    )
    .await;
    assert_eq!(tool["error"]["code"], INVALID_PARAMS);
}

#[tokio::test]
async fn tools_list_names_every_tool_with_a_schema() {
    let (server, _dir) = server(false);
    let reply = rpc(&server, json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/list" })).await;
    let names: Vec<&str> = reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            assert_eq!(t["inputSchema"]["type"], "object");
            t["name"].as_str().unwrap()
        })
        .collect();
    assert_eq!(names, ["list_datasources", "get_catalog", "run_query", "test_connection"]);
}

#[tokio::test]
async fn lists_sources_without_credentials() {
    let (server, _dir) = server(false);
    let (is_error, value) = call(&server, "list_datasources", json!({})).await;
    assert!(!is_error);
    assert_eq!(value["datasources"][0]["name"], "local db");
    assert_eq!(value["datasources"][0]["dialect"], "sqlite");
    assert_eq!(value["writes_allowed"], false);
    assert!(!value.to_string().contains("password"));
}

#[tokio::test]
async fn runs_a_read_query_and_reads_the_catalog() {
    let (server, _dir) = server(true);
    let ds = "local db";
    for sql in
        ["CREATE TABLE people (id INTEGER, name TEXT)", "INSERT INTO people VALUES (1, 'Ada')"]
    {
        let (is_error, v) =
            call(&server, "run_query", json!({ "datasource": ds, "sql": sql })).await;
        assert!(!is_error, "{v}");
    }
    let (is_error, v) = call(
        &server,
        "run_query",
        json!({ "datasource": ds, "sql": "SELECT id, name FROM people" }),
    )
    .await;
    assert!(!is_error, "{v}");
    assert_eq!(v["columns"].as_array().unwrap().len(), 2);
    assert_eq!(v["rows"][0][1], "Ada");
    assert_eq!(v["row_count"], 1);

    let (is_error, catalog) = call(&server, "get_catalog", json!({ "datasource": ds })).await;
    assert!(!is_error, "{catalog}");
    assert!(catalog.to_string().contains("people"));

    let (is_error, ok) = call(&server, "test_connection", json!({ "datasource": ds })).await;
    assert!(!is_error);
    assert_eq!(ok["ok"], true);
}

#[tokio::test]
async fn read_only_mode_refuses_writes_and_leaves_data_alone() {
    let (writable, dir) = server(true);
    let ds = "local db";
    call(&writable, "run_query", json!({ "datasource": ds, "sql": "CREATE TABLE t (a INTEGER)" }))
        .await;
    call(&writable, "run_query", json!({ "datasource": ds, "sql": "INSERT INTO t VALUES (1)" }))
        .await;

    // A second server over the same file, without write access.
    let mut registry = Registry::new();
    registry.register(rowcraft_dialect_sqlite::SqliteDialect);
    let locked = Server::new(
        Arc::new(Engine::new(registry)),
        Options { datasources: dir.0.join("datasources.toml"), allow_write: false },
    );
    let (is_error, message) =
        call(&locked, "run_query", json!({ "datasource": ds, "sql": "DELETE FROM t" })).await;
    assert!(is_error);
    assert!(message.as_str().unwrap().contains("--allow-write"), "{message}");
    let (is_error, v) =
        call(&locked, "run_query", json!({ "datasource": ds, "sql": "SELECT count(*) FROM t" }))
            .await;
    assert!(!is_error, "{v}");
    assert_eq!(v["rows"][0][0], 1);
}

#[tokio::test]
async fn failures_come_back_as_tool_errors_the_model_can_read() {
    let (server, _dir) = server(false);
    let (is_error, message) =
        call(&server, "run_query", json!({ "datasource": "missing", "sql": "SELECT 1" })).await;
    assert!(is_error);
    assert!(message.as_str().unwrap().contains("local db"), "lists the real names: {message}");

    let (is_error, _) = call(&server, "run_query", json!({ "datasource": "local db" })).await;
    assert!(is_error, "sql is required");

    let (is_error, _) = call(
        &server,
        "run_query",
        json!({ "datasource": "local db", "sql": "SELECT * FROM nope" }),
    )
    .await;
    assert!(is_error, "a statement error is a tool error");

    let (is_error, _) = call(
        &server,
        "run_query",
        json!({ "datasource": "local db", "sql": "SELECT 1", "max_rows": 0 }),
    )
    .await;
    assert!(is_error, "max_rows must be positive");
}
