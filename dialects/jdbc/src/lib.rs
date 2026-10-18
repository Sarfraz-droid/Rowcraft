//! Generic JDBC execution through an isolated Java runtime. No third-party JVM code is bundled.
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use dg_config::{DataSourceConfig, DriverFile, drivers_path};
use dg_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result,
    ResultSet, Value,
};
use dg_model::{Catalog, Column, Schema, Table, TableKind};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Copy)]
pub struct JdbcDialect {
    pub id: &'static str,
    pub name: &'static str,
    pub port: Option<u16>,
}
impl Default for JdbcDialect {
    fn default() -> Self {
        Self { id: "jdbc", name: "JDBC", port: None }
    }
}

#[async_trait]
impl Dialect for JdbcDialect {
    fn id(&self) -> DialectId {
        DialectId(self.id)
    }
    fn display_name(&self) -> &'static str {
        self.name
    }
    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }
    fn default_port(&self) -> Option<u16> {
        self.port
    }
    fn connects(&self) -> bool {
        true
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { transactions: true, ..Default::default() }
    }
    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        let cfg = cfg.clone();
        let conn = tokio::task::spawn_blocking(move || Process::connect(&cfg))
            .await
            .map_err(|e| Error::Connect(e.to_string()))??;
        Ok(Box::new(JdbcConnection { process: Arc::new(Mutex::new(conn)) }))
    }
    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        scope: &IntrospectScope,
    ) -> Result<Catalog> {
        let result = conn.query_limited("\0DG_JDBC_METADATA", usize::MAX).await?;
        let mut cat = Catalog::default();
        for row in result.rows {
            let text = |i: usize| match row.get(i) {
                Some(Value::Text(s)) => Some(s.clone()),
                _ => None,
            };
            if text(2).as_deref() == Some("DG_DATABASE") {
                cat.database = text(0);
                cat.default_schema = text(1).or_else(|| cat.database.clone());
                continue;
            }
            let schema = text(0).unwrap_or_else(|| "main".into());
            if scope.schemas.as_ref().is_some_and(|s| !s.contains(&schema)) {
                continue;
            }
            let table = text(1).unwrap_or_default();
            let si = match cat.schemas.iter().position(|s| s.name == schema) {
                Some(i) => i,
                None => {
                    cat.schemas.push(Schema { name: schema, ..Default::default() });
                    cat.schemas.len() - 1
                }
            };
            let tables = &mut cat.schemas[si].tables;
            let ti = match tables.iter().position(|t| t.name == table) {
                Some(i) => i,
                None => {
                    tables.push(Table {
                        name: table,
                        kind: if text(2).is_some_and(|s| s.contains("VIEW")) {
                            TableKind::View
                        } else {
                            TableKind::Table
                        },
                        ..Default::default()
                    });
                    tables.len() - 1
                }
            };
            if let Some(name) = text(3) {
                tables[ti].columns.push(Column {
                    name,
                    native_type: text(4).unwrap_or_default(),
                    nullable: row.get(5) == Some(&Value::Bool(true)),
                    default: text(6),
                    ..Default::default()
                });
            }
        }
        Ok(cat)
    }
}

#[derive(Debug)]
struct JdbcConnection {
    process: Arc<Mutex<Process>>,
}
#[async_trait]
impl Connection for JdbcConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let process = self.process.clone();
        let statement = statement.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut p =
                process.lock().map_err(|_| Error::Query("JDBC process lock poisoned".into()))?;
            let op = if statement == "\0DG_JDBC_METADATA" { "M" } else { "Q" };
            writeln!(
                p.input,
                "{}\t{}\t{}",
                op,
                STANDARD.encode(statement),
                max_rows.min(i32::MAX as usize)
            )
            .map_err(|e| Error::Query(e.to_string()))?;
            p.input.flush().map_err(|e| Error::Query(e.to_string()))?;
            let data = p.read()?;
            parse_result(data)
        })
        .await
        .map_err(|e| Error::Query(e.to_string()))?
    }
}
fn parse_result(data: serde_json::Value) -> Result<ResultSet> {
    let columns = data["columns"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let rows = data["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            row.as_array()
                .into_iter()
                .flatten()
                .map(|v| match v {
                    serde_json::Value::Null => Value::Null,
                    serde_json::Value::Bool(b) => Value::Bool(*b),
                    serde_json::Value::Number(n) => n
                        .as_i64()
                        .map(Value::Int)
                        .or_else(|| n.as_f64().map(Value::Float))
                        .unwrap_or(Value::Null),
                    serde_json::Value::String(s) => Value::Text(s.clone()),
                    _ => v["bytes"]
                        .as_str()
                        .and_then(|b| STANDARD.decode(b).ok())
                        .map(Value::Bytes)
                        .unwrap_or_else(|| Value::Text(v.to_string())),
                })
                .collect()
        })
        .collect();
    Ok(ResultSet {
        columns,
        rows,
        rows_affected: data["affected"].as_u64(),
        truncated: data["truncated"].as_bool().unwrap_or(false),
    })
}

struct Process {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    source_dir: PathBuf,
    secrets: Vec<String>,
}
impl std::fmt::Debug for Process {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JdbcProcess").field("pid", &self.child.id()).finish()
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.source_dir);
    }
}
impl Process {
    fn read(&mut self) -> Result<serde_json::Value> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.output.read_line(&mut line).map_err(|e| Error::Query(e.to_string()))? == 0 {
                return Err(Error::Query("JDBC runtime exited before returning a response; verify the Java runtime and driver JAR dependencies".into()));
            }
            if let Some(json) = line.strip_prefix("DG:") {
                let data: serde_json::Value =
                    serde_json::from_str(json).map_err(|e| Error::Query(e.to_string()))?;
                if let Some(error) = data["error"].as_str() {
                    let mut message = error.to_owned();
                    for secret in &self.secrets {
                        message = message.replace(secret, "<redacted>");
                    }
                    return Err(Error::Query(message));
                }
                return Ok(data);
            }
        }
    }
    fn connect(cfg: &DataSourceConfig) -> Result<Self> {
        let err = |e: std::io::Error| Error::Connect(e.to_string());
        let driver = cfg.driver.as_deref().unwrap_or(&cfg.dialect);
        let custom = DriverFile::load(&drivers_path())
            .ok()
            .and_then(|f| f.drivers.into_iter().find(|d| d.id == driver));
        let class = cfg
            .properties
            .get("jdbc.class")
            .cloned()
            .or_else(|| custom.as_ref().and_then(|d| d.jdbc_class.clone()))
            .unwrap_or_default();
        let mut jars = cfg
            .properties
            .get("jdbc.jars")
            .map(|p| std::env::split_paths(p).collect::<Vec<_>>())
            .unwrap_or_else(|| custom.as_ref().map(|d| d.jdbc_jars.clone()).unwrap_or_default());
        if jars.is_empty() {
            collect_jars(&dg_config::config_dir().join("jdbc").join(driver), &mut jars, 8);
        }
        if jars.is_empty() {
            discover_datagrip_jars(driver, &mut jars);
        }
        if jars.is_empty() {
            return Err(Error::Connect(format!(
                "No JDBC driver files for {driver}. Add driver JARs in Drivers, set jdbc.jars in Advanced, or place them in {}",
                dg_config::config_dir().join("jdbc").join(driver).display()
            )));
        }
        for jar in &jars {
            if !jar.is_file() {
                return Err(Error::Connect(format!(
                    "JDBC driver file does not exist: {}",
                    jar.display()
                )));
            }
        }
        let template = cfg
            .properties
            .get("jdbc.url_template")
            .map(String::as_str)
            .or_else(|| custom.as_ref().and_then(|d| d.jdbc_url_template.as_deref()));
        let url = connection_url(cfg, template)?;
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let source_dir = std::env::temp_dir().join(format!(
            "dglite-jdbc-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&source_dir).map_err(err)?;
        let source = source_dir.join("Bridge.java");
        std::fs::write(&source, include_str!("Bridge.java")).map_err(err)?;
        let java = java_runtime();
        let mut child = Command::new(java)
            .arg("--class-path")
            .arg(std::env::join_paths(&jars).map_err(|e| Error::Connect(e.to_string()))?)
            .arg(&source)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                Error::Connect(format!("Cannot start Java 11+: {e}. Set DGLITE_JAVA or JAVA_HOME."))
            })?;
        let input =
            child.stdin.take().ok_or_else(|| Error::Connect("JDBC stdin unavailable".into()))?;
        let output = BufReader::new(
            child.stdout.take().ok_or_else(|| Error::Connect("JDBC stdout unavailable".into()))?,
        );
        let secrets = cfg
            .password
            .iter()
            .chain(cfg.properties.get("password"))
            .filter(|s| !s.is_empty())
            .cloned()
            .collect();
        let mut process = Self { child, input, output, source_dir, secrets };
        write!(process.input, "{}\t{}", STANDARD.encode(class), STANDARD.encode(url))
            .map_err(err)?;
        let mut props = cfg.properties.clone();
        props.retain(|k, _| !k.starts_with("jdbc."));
        if let Some(u) = &cfg.user {
            props.insert("user".into(), u.clone());
        }
        if let Some(p) = &cfg.password {
            props.insert("password".into(), p.clone());
        }
        props.insert("_dglite.readOnly".into(), cfg.read_only.to_string());
        props.insert(
            "_dglite.manual".into(),
            (cfg.options.transaction == dg_config::TransactionMode::Manual).to_string(),
        );
        let isolation = match cfg.options.isolation {
            dg_config::Isolation::ReadUncommitted => Some(1),
            dg_config::Isolation::ReadCommitted => Some(2),
            dg_config::Isolation::RepeatableRead => Some(4),
            dg_config::Isolation::Serializable => Some(8),
            _ => None,
        };
        if let Some(i) = isolation {
            props.insert("_dglite.isolation".into(), i.to_string());
        }
        for (k, v) in props {
            write!(process.input, "\t{}\t{}", STANDARD.encode(k), STANDARD.encode(v))
                .map_err(err)?;
        }
        writeln!(process.input).map_err(err)?;
        process.input.flush().map_err(err)?;
        process.read().map_err(|e| {
            let mut message = e.to_string();
            if let Some(secret) = cfg.password.as_deref().filter(|p| !p.is_empty()) {
                message = message.replace(secret, "<redacted>");
            }
            Error::Connect(message)
        })?;
        Ok(process)
    }
}
fn java_runtime() -> PathBuf {
    if let Some(p) = std::env::var_os("DGLITE_JAVA") {
        return p.into();
    }
    if let Some(p) = std::env::var_os("JAVA_HOME") {
        let home = PathBuf::from(p);
        let java = home.join("bin/java");
        let release = std::fs::read_to_string(home.join("release")).unwrap_or_default();
        let major = release
            .lines()
            .find_map(|line| line.strip_prefix("JAVA_VERSION=\""))
            .and_then(|v| v.split('.').next())
            .and_then(|v| v.parse::<u32>().ok());
        if java.is_file() && major.is_some_and(|v| v >= 11) {
            return java;
        }
    }
    let bundled = PathBuf::from("/Applications/DataGrip.app/Contents/jbr/Contents/Home/bin/java");
    if bundled.is_file() { bundled } else { PathBuf::from("java") }
}
fn collect_jars(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth == 0 {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_jars(&path, out, depth - 1);
            } else if path.extension().is_some_and(|e| e == "jar") {
                out.push(path);
            }
        }
    }
}
fn discover_datagrip_jars(driver: &str, out: &mut Vec<PathBuf>) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let base = PathBuf::from(home).join("Library/Application Support/JetBrains");
    let display = dg_drivers::find(driver).map(|d| d.name).unwrap_or(driver).to_lowercase();
    let mut versions = std::fs::read_dir(base)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|s| s.to_string_lossy().starts_with("DataGrip")))
        .collect::<Vec<_>>();
    versions.sort();
    versions.reverse();
    for version in versions {
        if let Ok(entries) = std::fs::read_dir(version.join("jdbc-drivers")) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                let matched = name == display
                    || name.starts_with(&display)
                    || (driver == "sqlite" && name.contains("sqlite"));
                if matched {
                    let mut releases = std::fs::read_dir(entry.path())
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(|e| e.path())
                        .collect::<Vec<_>>();
                    releases.sort();
                    if let Some(release) = releases.pop() {
                        collect_jars(&release, out, 12);
                    }
                }
            }
            if !out.is_empty() {
                return;
            }
        }
    }
}
pub fn connection_url(cfg: &DataSourceConfig, template: Option<&str>) -> Result<String> {
    if let Some(url) = cfg.url.as_deref().filter(|s| s.starts_with("jdbc:")) {
        return Ok(url.to_owned());
    }
    let driver = cfg.driver.as_deref().unwrap_or(&cfg.dialect);
    let builtin = match driver {
        // Modern Java has no bundled JDBC/ODBC bridge; users supply a bridge JAR.
        "odbc" => "jdbc:odbc:{database}",
        "oracle" => "jdbc:oracle:thin:@//{host}:{port}/{database}",
        "derby_embedded" => "jdbc:derby:{database};create=true",
        "derby_remote" | "derby" => "jdbc:derby://{host}:{port}/{database}",
        "hsqldb_local" => "jdbc:hsqldb:file:{database}",
        "hsqldb_remote" | "hsqldb" => "jdbc:hsqldb:hsql://{host}:{port}/{database}",
        "duckdb" => "jdbc:duckdb:{database}",
        "athena" => "jdbc:athena://AwsRegion={database}",
        "ignite" => "jdbc:ignite:thin://{host}:{port}",
        "phoenix_thick" => "jdbc:phoenix:{host}:{port}",
        "phoenix_thin" => "jdbc:phoenix:thin:url=http://{host}:{port}",
        "databricks" => "jdbc:databricks://{host}:{port};httpPath={database}",
        "denodo" => "jdbc:vdb://{host}:{port}/{database}",
        "elasticsearch" => "jdbc:es://https://{host}:{port}",
        "firebird" => "jdbc:firebirdsql://{host}:{port}/{database}",
        "cloudspanner" => "jdbc:cloudspanner:/{database}",
        "informix" => "jdbc:informix-sqli://{host}:{port}/{database}",
        "influxdb" => "jdbc:arrow-flight-sql://{host}:{port}?database={database}",
        "iris" => "jdbc:IRIS://{host}:{port}/{database}",
        "mimer" => "jdbc:mimer:multi1:{database}",
        "openedge" => "jdbc:datadirect:openedge://{host}:{port};databaseName={database}",
        "hana" => "jdbc:sap://{host}:{port}/?databaseName={database}",
        "tarantool" => "jdbc:tarantool://{host}:{port}/{database}",
        "teradata" => "jdbc:teradata://{host}/DATABASE={database},DBS_PORT={port}",
        "tibero" => "jdbc:tibero:thin:@{host}:{port}:{database}",
        "db2" => "jdbc:db2://{host}:{port}/{database}",
        "db2_jtopen" => "jdbc:as400://{host}/{database}",
        "sybase" => "jdbc:sybase:Tds:{host}:{port}/{database}",
        "vertica" => "jdbc:vertica://{host}:{port}/{database}",
        "exasol" => "jdbc:exa:{host}:{port};schema={database}",
        "hive" => "jdbc:hive2://{host}:{port}/{database}",
        "spark" => "jdbc:hive2://{host}:{port}/{database}",
        "snowflake" => "jdbc:snowflake://{host}:{port}/?db={database}",
        "bigquery" => "jdbc:bigquery://https://{host}:{port};ProjectId={database}",
        _ => "",
    };
    let template = template.unwrap_or(builtin);
    if template.is_empty() {
        return Err(Error::Connect(
            "Enter a JDBC URL or configure jdbc.url_template for this driver".into(),
        ));
    }
    let port = cfg
        .port
        .or_else(|| dg_drivers::find(driver).and_then(|d| d.port))
        .map(|p| p.to_string())
        .unwrap_or_default();
    if template.contains("{port}") && port.is_empty() {
        return Err(Error::Connect("This JDBC driver needs a port or a complete JDBC URL".into()));
    }
    Ok(template
        .replace("{host}", cfg.host.as_deref().unwrap_or("localhost"))
        .replace("{port}", &port)
        .replace("{database}", cfg.database.as_deref().unwrap_or(""))
        .replace("{user}", cfg.user.as_deref().unwrap_or("")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_vendor_and_custom_urls() {
        let mut cfg = DataSourceConfig {
            driver: Some("oracle".into()),
            host: Some("db.example".into()),
            database: Some("ORCL".into()),
            ..Default::default()
        };
        assert_eq!(connection_url(&cfg, None).unwrap(), "jdbc:oracle:thin:@//db.example:1521/ORCL");
        cfg.port = Some(1234);
        assert_eq!(
            connection_url(&cfg, Some("jdbc:custom://{host}:{port}/{database}")).unwrap(),
            "jdbc:custom://db.example:1234/ORCL"
        );
        cfg.url = Some("jdbc:opaque:in-memory".into());
        assert_eq!(connection_url(&cfg, None).unwrap(), "jdbc:opaque:in-memory");
    }
    #[test]
    fn vendor_templates_never_generate_empty_required_ports() {
        for driver in dg_drivers::all().iter().filter(|d| !d.custom) {
            let cfg = DataSourceConfig {
                driver: Some(driver.id.into()),
                database: Some("sample".into()),
                ..Default::default()
            };
            match connection_url(&cfg, None) {
                Ok(url) => {
                    assert!(!url.contains("localhost:/"), "{}: {}", driver.id, url);
                    assert!(!url.contains("localhost:;"), "{}: {}", driver.id, url);
                    assert!(!url.ends_with("localhost:"), "{}: {}", driver.id, url);
                    assert!(!url.contains("{port}"), "{}: {}", driver.id, url);
                }
                Err(error) => assert!(
                    error.to_string().contains("needs a port")
                        || error.to_string().contains("Enter a JDBC URL"),
                    "{}: {}",
                    driver.id,
                    error
                ),
            }
        }
        let cfg = DataSourceConfig {
            driver: Some("odbc".into()),
            database: Some("MyDSN".into()),
            ..Default::default()
        };
        assert_eq!(connection_url(&cfg, None).unwrap(), "jdbc:odbc:MyDSN");
    }

    #[test]
    fn decodes_null_binary_numeric_and_truncation() {
        let data = serde_json::json!({"columns":["n","b","i","f","s","bytes"],"rows":[[null,true,7,1.25,"value",{"bytes":"AAH/"}]],"truncated":true});
        let r = parse_result(data).unwrap();
        assert_eq!(
            r.rows[0],
            vec![
                Value::Null,
                Value::Bool(true),
                Value::Int(7),
                Value::Float(1.25),
                Value::Text("value".into()),
                Value::Bytes(vec![0, 1, 255])
            ]
        );
        assert!(r.truncated);
    }
    /// Opt-in real Java/JDBC test, independent of an external database server.
    #[tokio::test]
    async fn sqlite_jdbc_session_query_metadata_and_errors() {
        let Ok(jar) = std::env::var("DGLITE_TEST_JDBC_JAR") else {
            return;
        };
        let mut cfg = DataSourceConfig {
            dialect: "jdbc".into(),
            url: Some("jdbc:sqlite::memory:".into()),
            ..Default::default()
        };
        cfg.properties.insert("jdbc.jars".into(), jar);
        cfg.properties.insert("jdbc.class".into(), "org.sqlite.JDBC".into());
        let dialect = JdbcDialect::default();
        let mut conn = dialect.connect(&cfg).await.unwrap();
        conn.query("create table people(id integer, name text)").await.unwrap();
        conn.query("insert into people values(1,'Ada'),(2,'Grace'),(3,null)").await.unwrap();
        let r = conn.query_limited("select * from people order by id", 2).await.unwrap();
        assert_eq!(r.columns, ["id", "name"]);
        assert_eq!(r.rows.len(), 2);
        assert!(r.truncated);
        assert_eq!(r.rows[0], [Value::Int(1), Value::Text("Ada".into())]);
        assert!(conn.query("select * from missing_table").await.is_err());
        assert_eq!(conn.query("select 42").await.unwrap().rows[0], [Value::Int(42)]);
        let catalog = dialect.introspect(conn.as_mut(), &IntrospectScope::default()).await.unwrap();
        assert_eq!(catalog.table("main", "people").unwrap().columns.len(), 2);
    }
}
