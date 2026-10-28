#![allow(clippy::unwrap_used)]

use super::*;

use http_body_util::BodyExt;
use rowcraft_dialect::Registry;

/// Removes the test's directory when dropped.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A data sources file with one SQLite source (`local db`) in a fresh directory.
fn api(token: Option<&str>) -> (Api, TempDir) {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rowcraft-api-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let datasources = dir.join("datasources.toml");
    let file = DataSourceFile {
        datasources: vec![DataSourceConfig {
            name: "local db".into(),
            dialect: "sqlite".into(),
            database: Some(dir.join("t.db").display().to_string()),
            ..Default::default()
        }],
    };
    file.save(&datasources).unwrap();
    let mut registry = Registry::new();
    registry.register(rowcraft_dialect_sqlite::SqliteDialect);
    let options = Options {
        addr: "127.0.0.1:0".parse().unwrap(),
        token: token.map(str::to_owned),
        datasources,
    };
    (Api::new(Arc::new(Engine::new(registry)), options), TempDir(dir))
}

fn request(method: &str, uri: &str) -> hyper::http::request::Builder {
    Request::builder().method(method).uri(uri).header(header::HOST, "localhost:7878")
}

async fn send(api: &Api, req: Request<Bytes>) -> (StatusCode, String) {
    let reply = api.handle(req).await;
    let status = reply.status();
    let body = reply.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

async fn send_json(api: &Api, req: Request<Bytes>) -> (StatusCode, serde_json::Value) {
    let (status, body) = send(api, req).await;
    (status, serde_json::from_str(&body).unwrap())
}

fn sql(uri: &str, body: &str) -> Request<Bytes> {
    request("POST", uri).body(Bytes::from(body.to_owned())).unwrap()
}

#[tokio::test]
async fn lists_every_saved_source_with_its_endpoints() {
    let (api, _dir) = api(None);
    let (status, list) =
        send_json(&api, request("GET", "/datasources").body(Bytes::new()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list[0]["name"], "local db");
    assert_eq!(list[0]["dialect"], "sqlite");
    assert_eq!(list[0]["endpoints"]["query"], "POST /datasources/local%20db/query");
}

#[tokio::test]
async fn runs_sql_from_plain_text_json_and_query_string() {
    let (api, _dir) = api(None);
    let (status, created) = send_json(
        &api,
        sql("/datasources/local%20db/query", "create table t(id integer, name text, score real)"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    send_json(
        &api,
        sql("/datasources/local%20db/query", "insert into t values (1, 'a', 1.5), (2, null, 2)"),
    )
    .await;

    let json_body = request("POST", "/datasources/local%20db/query")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Bytes::from(r#"{"sql": "select * from t order by id", "max_rows": 1}"#))
        .unwrap();
    let (status, rs) = send_json(&api, json_body).await;
    assert_eq!(status, StatusCode::OK, "{rs}");
    assert_eq!(rs["columns"], json!(["id", "name", "score"]));
    assert_eq!(rs["rows"], json!([[1, "a", 1.5]]));
    assert_eq!(rs["truncated"], true);

    let (status, rs) =
        send_json(&api, sql("/datasources/local%20db/query?sql=select+count(*)+as+n+from+t", ""))
            .await;
    assert_eq!(status, StatusCode::OK, "{rs}");
    assert_eq!(rs["rows"], json!([[2]]));
}

#[tokio::test]
async fn other_formats_come_from_the_exporters() {
    let (api, _dir) = api(None);
    let (status, csv) =
        send(&api, sql("/datasources/local%20db/query?format=csv", "select 1 as a, 'x' as b"))
            .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(csv.lines().collect::<Vec<_>>(), ["a,b", "1,x"]);
    let (status, err) =
        send_json(&api, sql("/datasources/local%20db/query?format=xml", "select 1")).await;
    assert_eq!((status, err["kind"].as_str()), (StatusCode::BAD_REQUEST, Some("usage")));
}

#[tokio::test]
async fn errors_carry_a_status_and_kind() {
    let (api, _dir) = api(None);
    let (status, err) = send_json(&api, sql("/datasources/nope/query", "select 1")).await;
    assert_eq!((status, err["kind"].as_str()), (StatusCode::NOT_FOUND, Some("not_found")));

    let (status, err) =
        send_json(&api, sql("/datasources/local%20db/query", "select * from missing")).await;
    assert_eq!(
        (status, err["kind"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("statement")),
        "{err}"
    );

    let (status, _) = send_json(&api, sql("/datasources/local%20db/query", "  ")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send_json(
        &api,
        request("GET", "/datasources/local%20db/query").body(Bytes::new()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "SQL is never run from a GET");
}

#[tokio::test]
async fn catalog_test_and_session_lifecycle() {
    let (api, _dir) = api(None);
    send_json(&api, sql("/datasources/local%20db/query", "create table things(id integer)")).await;
    let (status, catalog) = send_json(
        &api,
        request("GET", "/datasources/local%20db/catalog").body(Bytes::new()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{catalog}");
    assert!(catalog.to_string().contains("things"), "{catalog}");

    let (status, ok) = send_json(&api, sql("/datasources/local%20db/test", "")).await;
    assert_eq!((status, ok["ok"].as_bool()), (StatusCode::OK, Some(true)));

    let (_, idle) = send_json(&api, sql("/datasources/local%20db/cancel", "")).await;
    assert_eq!(idle["cancelled"], false);

    let disconnect = || request("DELETE", "/datasources/local%20db/session").body(Bytes::new());
    let (_, closed) = send_json(&api, disconnect().unwrap()).await;
    assert_eq!(closed["closed"], true);
    let (_, closed) = send_json(&api, disconnect().unwrap()).await;
    assert_eq!(closed["closed"], false);
}

#[tokio::test]
async fn refuses_browsers_foreign_hosts_and_missing_tokens() {
    let (api, _dir) = api(Some("s3cret"));
    let health = || request("GET", "/health");

    let (status, _) = send(&api, health().body(Bytes::new()).unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(
        &api,
        health().header(header::AUTHORIZATION, "Bearer wrong").body(Bytes::new()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(
        &api,
        health().header(header::AUTHORIZATION, "Bearer s3cret").body(Bytes::new()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send(
        &api,
        health()
            .header(header::AUTHORIZATION, "Bearer s3cret")
            .header(header::ORIGIN, "https://evil.example")
            .body(Bytes::new())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let rebound = Request::builder()
        .uri("/health")
        .header(header::HOST, "evil.example")
        .header(header::AUTHORIZATION, "Bearer s3cret")
        .body(Bytes::new())
        .unwrap();
    assert_eq!(send(&api, rebound).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn will_not_listen_publicly_without_a_token() {
    let (api, _dir) = api(None);
    let options = Options { addr: "0.0.0.0:0".parse().unwrap(), ..api.options.clone() };
    let err = serve(api.engine.clone(), options).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn hosts_and_percent_coding() {
    for ok in ["localhost", "localhost:80", "127.0.0.1:7878", "[::1]:7878", "LOCALHOST"] {
        assert!(is_loopback_host(ok), "{ok}");
    }
    for bad in ["", "evil.example", "127.0.0.1.evil.example:80", "10.0.0.1"] {
        assert!(!is_loopback_host(bad), "{bad}");
    }
    assert_eq!(percent_decode("a%20b%2Fc%zz"), "a b/c%zz");
    assert_eq!(percent_encode("a b/c"), "a%20b%2Fc");
    assert_eq!(param(&parse_query("x=1&sql=select+1%2B1"), "sql"), Some("select 1+1"));
}
