//! UI-independent engine. Depends on dialect *traits* only; the caller supplies a `Registry`.

use std::sync::Arc;

use std::path::PathBuf;

use rowcraft_config::{
    AuthType, DataSourceConfig, Defaults, ObjectFilter, SchemaPattern, SshConfig, SshFile, ssh_path,
};
use rowcraft_dialect::{Connection, Dialect, IntrospectScope, Registry, ResultSet};
use rowcraft_model::{Catalog, TableKind};
use rowcraft_ssh::{
    Credentials, EnvCredentials, RusshOpener, TunnelOpener, Tunnels, parse_targets,
};
use tokio::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] rowcraft_config::Error),
    #[error(transparent)]
    Dialect(#[from] rowcraft_dialect::Error),
    #[error(transparent)]
    Ssh(#[from] rowcraft_ssh::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub struct Engine {
    registry: Registry,
    tunnels: Arc<dyn TunnelOpener>,
    credentials: Arc<dyn Credentials>,
    ssh_file: PathBuf,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("registry", &self.registry)
            .field("ssh_file", &self.ssh_file)
            .finish_non_exhaustive()
    }
}

impl Engine {
    pub fn new(registry: Registry) -> Self {
        Self {
            registry,
            tunnels: Arc::new(RusshOpener),
            credentials: Arc::new(EnvCredentials),
            ssh_file: ssh_path(),
        }
    }

    /// Replaces how SSH tunnels are opened (tests; the real opener is the default).
    pub fn with_tunnel_opener(mut self, opener: Arc<dyn TunnelOpener>) -> Self {
        self.tunnels = opener;
        self
    }

    /// Replaces where SSH passwords and key passphrases come from (the GUI prompts).
    pub fn with_credentials(mut self, credentials: Arc<dyn Credentials>) -> Self {
        self.credentials = credentials;
        self
    }

    /// Reads shared SSH configurations from `path` instead of `ssh.toml` in the config directory.
    pub fn with_ssh_file(mut self, path: PathBuf) -> Self {
        self.ssh_file = path;
        self
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Opens a connection the way the data source describes it: the URL overrides host, port,
    /// database and user; the authentication type decides whether a password is sent; the password
    /// comes from `password_env`; the startup script runs once connected.
    pub async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        self.connect_with(cfg, None).await
    }

    /// Like [`Engine::connect`], but `ssh` stands in for the saved SSH configuration of the same
    /// name, so a configuration that is still being edited can be tried before it is saved.
    pub async fn connect_with(
        &self,
        cfg: &DataSourceConfig,
        ssh: Option<&SshConfig>,
    ) -> Result<Box<dyn Connection>> {
        let dialect = self.registry.get(&cfg.dialect)?;
        let mut cfg = cfg.effective()?;
        match cfg.auth {
            AuthType::UserPassword => {}
            AuthType::NoAuth => {
                cfg.password_env = None;
                cfg.password = None;
            }
            AuthType::WindowsCredentials => {
                return Err(rowcraft_dialect::Error::NotImplemented("Windows credentials").into());
            }
        }
        cfg.resolve_password()?;
        let tunnel_name =
            cfg.ssh_tunnel.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned);
        let tunnels = match tunnel_name {
            Some(name) => {
                let jdbc_url =
                    cfg.url.as_deref().filter(|u| u.starts_with("jdbc:")).map(str::to_owned);
                if let Some(url) = &jdbc_url {
                    if rowcraft_config::parse_url(url)
                        .map_err(rowcraft_config::Error::InvalidUrl)?
                        .host
                        .is_none_or(|h| h.contains(','))
                    {
                        return Err(rowcraft_config::Error::InvalidUrl("SSH tunneling with an opaque JDBC URL requires using the Host/Port fields and a driver URL template instead.".into()).into());
                    }
                }
                let tunnels =
                    self.open_tunnels(&mut cfg, name, ssh, dialect.default_port()).await?;
                if let Some(url) = jdbc_url {
                    // JDBC consumes the URL itself, so point its authority at the forwarded port.
                    let (prefix, rest) = url.split_once("://").ok_or_else(|| {
                        rowcraft_config::Error::InvalidUrl(
                            "JDBC URL has no network authority.".into(),
                        )
                    })?;
                    let end = rest.find(['/', '?', ';']).unwrap_or(rest.len());
                    let authority = &rest[..end];
                    let user = authority.rsplit_once('@').map_or("", |(u, _)| u);
                    let auth = if user.is_empty() { String::new() } else { format!("{user}@") };
                    cfg.url = Some(format!(
                        "{prefix}://{auth}{}:{}{}",
                        cfg.host.as_deref().unwrap_or("127.0.0.1"),
                        cfg.port.unwrap_or_default(),
                        &rest[end..]
                    ));
                }
                Some(tunnels)
            }
            None => None,
        };
        let mut conn = dialect.connect(&cfg).await?;
        if let Some(script) = cfg.options.startup_script.as_deref().filter(|s| !s.trim().is_empty())
        {
            conn.query(script).await.map_err(|e| match e {
                rowcraft_dialect::Error::Query(m) => {
                    rowcraft_dialect::Error::Query(format!("startup script failed: {m}"))
                }
                other => other,
            })?;
        }
        Ok(match tunnels {
            // The tunnel must live exactly as long as the connection that uses it.
            Some(tunnels) => Box::new(Tunneled { inner: conn, _tunnels: tunnels }),
            None => conn,
        })
    }

    /// Opens a tunnel for each host:port of the data source and points `cfg` at the local ends.
    async fn open_tunnels(
        &self,
        cfg: &mut DataSourceConfig,
        name: String,
        unsaved: Option<&SshConfig>,
        default_port: Option<u16>,
    ) -> Result<Tunnels> {
        let ssh = match unsaved.filter(|c| c.name == name) {
            Some(c) => c.clone(),
            None => SshFile::load(&self.ssh_file)?.get(&name)?.clone(),
        };
        let host = cfg.host.clone().unwrap_or_else(|| "localhost".to_owned());
        let targets = parse_targets(&host, cfg.port, default_port)?;
        let tunnels = self
            .tunnels
            .open(&ssh, self.credentials.as_ref(), &targets, cfg.ssh_local_port)
            .await?;
        match tunnels.endpoints() {
            [one] => {
                cfg.host = Some(one.ip().to_string());
                cfg.port = Some(one.port());
            }
            many => {
                let list: Vec<String> = many.iter().map(ToString::to_string).collect();
                cfg.host = Some(list.join(","));
                cfg.port = None;
            }
        }
        Ok(tunnels)
    }

    /// Like [`Engine::connect`], but wraps the connection in a shareable [`Session`].
    pub async fn open_session(&self, cfg: &DataSourceConfig) -> Result<Arc<Session>> {
        // Parse the Schemas tab first so a typo fails before a connection is opened.
        let scope = CatalogScope {
            schemas: cfg.parsed_schema_pattern()?,
            objects: cfg.parsed_object_filter()?,
        };
        let conn = self.connect(cfg).await?;
        let dialect = self.registry.get(&cfg.dialect)?;
        Ok(Arc::new(Session {
            name: cfg.name.clone(),
            dialect,
            scope,
            cfg: cfg.clone(),
            conn: Mutex::new(conn),
        }))
    }

    /// Connects, reads the catalog and disconnects.
    pub async fn introspect(
        &self,
        cfg: &DataSourceConfig,
        scope: &IntrospectScope,
    ) -> Result<Catalog> {
        self.open_session(cfg).await?.introspect(scope).await
    }
}

/// A connection that keeps its SSH tunnels open for as long as it exists.
#[derive(Debug)]
struct Tunneled {
    inner: Box<dyn Connection>,
    _tunnels: Tunnels,
}

#[async_trait::async_trait]
impl Connection for Tunneled {
    async fn query_limited(
        &mut self,
        statement: &str,
        max_rows: usize,
    ) -> rowcraft_dialect::Result<ResultSet> {
        self.inner.query_limited(statement, max_rows).await
    }
}

/// One open connection, shared between the UI and background tasks. Statements on a session run
/// one at a time; a second one waits for the first.
pub struct Session {
    name: String,
    dialect: Arc<dyn Dialect>,
    scope: CatalogScope,
    /// What the session was opened from, so a dropped connection can be opened again.
    cfg: DataSourceConfig,
    conn: Mutex<Box<dyn Connection>>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("name", &self.name)
            .field("dialect", &self.dialect.id())
            .finish_non_exhaustive()
    }
}

impl Session {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub async fn query(&self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        Ok(self.conn.lock().await.query_limited(statement, max_rows).await?)
    }

    /// Keeps session setup and execution under the same connection lock.
    pub async fn query_with_setup(
        &self,
        setup: &str,
        statement: &str,
        max_rows: usize,
    ) -> Result<ResultSet> {
        let mut conn = self.conn.lock().await;
        conn.query(setup).await?;
        Ok(conn.query_limited(statement, max_rows).await?)
    }

    /// Reads the catalog and applies the data source's schema pattern and object filter.
    pub async fn introspect(&self, scope: &IntrospectScope) -> Result<Catalog> {
        let mut conn = self.conn.lock().await;
        let catalog = self.dialect.introspect(conn.as_mut(), scope).await?;
        Ok(self.scope.apply(catalog))
    }

    /// Replaces the connection with a fresh one (tunnels and startup script included), e.g. after
    /// the server closed it for being idle. Statements queued behind the lock use the new one.
    pub async fn reconnect(&self, engine: &Engine) -> Result<()> {
        let fresh = engine.connect(&self.cfg).await?;
        *self.conn.lock().await = fresh;
        Ok(())
    }

    /// Runs `statements` in order without letting another statement in between. With `tx`
    /// (`[begin, commit, rollback]`) they run as one transaction that a failure rolls back.
    /// `check` sees each statement's result and can reject it, which also counts as a failure.
    /// Returns the total of the affected-row counts the database reported.
    pub async fn execute_all(
        &self,
        tx: Option<[&str; 3]>,
        statements: &[String],
        check: impl Fn(usize, &ResultSet) -> Result<(), String>,
    ) -> Result<u64> {
        let mut conn = self.conn.lock().await;
        if let Some([begin, ..]) = tx {
            conn.query(begin).await?;
        }
        let mut affected = 0;
        let mut outcome = Ok(());
        for (i, statement) in statements.iter().enumerate() {
            match conn.query(statement).await {
                Ok(rs) => {
                    affected += rs.rows_affected.unwrap_or(0);
                    if let Err(message) = check(i, &rs) {
                        outcome = Err(rowcraft_dialect::Error::Query(message));
                        break;
                    }
                }
                Err(e) => {
                    outcome = Err(e);
                    break;
                }
            }
        }
        match (outcome, tx) {
            (Ok(()), Some([_, commit, _])) => {
                conn.query(commit).await?;
            }
            (Ok(()), None) => {}
            (Err(e), Some([.., rollback])) => {
                // The original error is the useful one; a failed rollback (e.g. the connection
                // dropped) leaves nothing committed either.
                let _ = conn.query(rollback).await;
                return Err(e.into());
            }
            (Err(e), None) => return Err(e.into()),
        }
        Ok(affected)
    }
}

/// Whether `error` means the connection itself is gone (the server closed it after a timeout,
/// the network or SSH tunnel dropped, …) rather than the statement being wrong. Drivers only
/// report this as text, so it is recognised by the phrases they use.
pub fn is_connection_lost(error: &Error) -> bool {
    const PHRASES: &[&str] = &[
        "connection closed",
        "connection to the server is closed",
        "connection reset",
        "connection aborted",
        "broken pipe",
        "unexpected eof",
        "unexpected end of file",
        "not connected",
        "server has gone away",
        "lost connection",
        "disconnected by the server",
        "error communicating with the server",
        "input/output error",
        "pool was disconnected",
        "socket closed",
        "channel closed",
    ];
    let message = error.to_string().to_lowercase();
    PHRASES.iter().any(|phrase| message.contains(phrase))
}

/// The Schemas tab of a data source: what introspection results to keep.
#[derive(Debug, Default)]
struct CatalogScope {
    schemas: Option<SchemaPattern>,
    objects: Option<ObjectFilter>,
}

/// Object kind names an object filter's `type:` prefix refers to.
fn table_kind_key(kind: TableKind) -> &'static str {
    match kind {
        TableKind::Table | TableKind::PartitionedTable | TableKind::ForeignTable => "table",
        TableKind::View => "view",
        TableKind::MaterializedView => "materialized view",
    }
}

impl CatalogScope {
    fn apply(&self, mut catalog: Catalog) -> Catalog {
        let database = catalog.database.clone().unwrap_or_default();
        let default_schema = catalog.default_schema.clone();
        if let Some(pattern) = &self.schemas {
            let defaults =
                Defaults { database: Some(&database), schema: default_schema.as_deref() };
            catalog.schemas.retain(|s| pattern.matches(&database, &s.name, defaults));
        }
        if let Some(filter) = &self.objects {
            for schema in &mut catalog.schemas {
                schema.tables.retain(|t| filter.allows(table_kind_key(t.kind), &t.name));
                schema.sequences.retain(|s| filter.allows("sequence", &s.name));
                schema.routines.retain(|r| filter.allows("routine", &r.name));
            }
        }
        catalog
    }
}

// TODO(M1): console state, transactions, QueryJob + cancellation, history, event stream.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;
    use rowcraft_config::{AuthType, ConnectionOptions};
    use rowcraft_dialect::{Dialect, DialectFamily, DialectId, Value};
    use rowcraft_model::{Routine, Schema, Sequence, Table};

    use super::*;

    /// What the fake database saw.
    #[derive(Debug, Default)]
    struct Log {
        statements: StdMutex<Vec<String>>,
        connects: StdMutex<Vec<DataSourceConfig>>,
    }

    impl Log {
        fn statements(&self) -> Vec<String> {
            self.statements.lock().unwrap().clone()
        }
        fn last_connect(&self) -> DataSourceConfig {
            self.connects.lock().unwrap().last().cloned().expect("a connection was made")
        }
    }

    #[derive(Debug)]
    struct FakeConnection(Arc<Log>);

    #[async_trait]
    impl Connection for FakeConnection {
        async fn query_limited(
            &mut self,
            statement: &str,
            max_rows: usize,
        ) -> rowcraft_dialect::Result<ResultSet> {
            self.0.statements.lock().unwrap().push(statement.to_owned());
            if statement == "boom" || statement == "bad script" {
                return Err(rowcraft_dialect::Error::Query(statement.into()));
            }
            let all: Vec<Vec<Value>> = (0..5).map(|i| vec![Value::Int(i)]).collect();
            Ok(ResultSet {
                columns: vec!["n".into()],
                truncated: all.len() > max_rows,
                rows: all.into_iter().take(max_rows).collect(),
                rows_affected: None,
            })
        }
    }

    struct Fake(Arc<Log>);

    fn table(name: &str, kind: TableKind) -> Table {
        Table { name: name.into(), kind, ..Default::default() }
    }

    /// Two schemas in database `app`, whose default schema is `public`.
    fn sample_catalog() -> Catalog {
        Catalog {
            database: Some("app".into()),
            default_schema: Some("public".into()),
            schemas: vec![
                Schema {
                    name: "public".into(),
                    tables: vec![
                        table("orders", TableKind::Table),
                        table("tmp_scratch", TableKind::Table),
                        table("v_orders", TableKind::View),
                    ],
                    sequences: vec![Sequence { name: "orders_seq".into(), ..Default::default() }],
                    routines: vec![Routine { name: "tmp_fn".into(), ..Default::default() }],
                    ..Default::default()
                },
                Schema {
                    name: "audit".into(),
                    tables: vec![table("log", TableKind::Table)],
                    ..Default::default()
                },
            ],
        }
    }

    #[async_trait]
    impl Dialect for Fake {
        fn id(&self) -> DialectId {
            DialectId("fake")
        }
        fn display_name(&self) -> &'static str {
            "Fake"
        }
        fn family(&self) -> DialectFamily {
            DialectFamily::Sql
        }
        async fn connect(
            &self,
            cfg: &DataSourceConfig,
        ) -> rowcraft_dialect::Result<Box<dyn Connection>> {
            self.0.connects.lock().unwrap().push(cfg.clone());
            Ok(Box::new(FakeConnection(self.0.clone())))
        }
        async fn introspect(
            &self,
            conn: &mut dyn Connection,
            scope: &IntrospectScope,
        ) -> rowcraft_dialect::Result<Catalog> {
            // Proves the session hands its own connection to the dialect.
            conn.query("introspect").await?;
            Ok(match &scope.schemas {
                Some(names) => Catalog {
                    schemas: names
                        .iter()
                        .map(|name| Schema { name: name.clone(), ..Default::default() })
                        .collect(),
                    ..Default::default()
                },
                None => sample_catalog(),
            })
        }
    }

    /// A dialect that leaves `introspect` at its default.
    struct Bare;

    #[async_trait]
    impl Dialect for Bare {
        fn id(&self) -> DialectId {
            DialectId("bare")
        }
        fn display_name(&self) -> &'static str {
            "Bare"
        }
        fn family(&self) -> DialectFamily {
            DialectFamily::Sql
        }
        async fn connect(
            &self,
            _: &DataSourceConfig,
        ) -> rowcraft_dialect::Result<Box<dyn Connection>> {
            Ok(Box::new(FakeConnection(Arc::default())))
        }
    }

    fn engine_with_log() -> (Engine, Arc<Log>) {
        let log = Arc::new(Log::default());
        let mut registry = Registry::new();
        registry.register(Fake(log.clone()));
        registry.register(Bare);
        (Engine::new(registry), log)
    }

    fn engine() -> Engine {
        engine_with_log().0
    }

    fn cfg(dialect: &str) -> DataSourceConfig {
        DataSourceConfig { name: "t".into(), dialect: dialect.into(), ..Default::default() }
    }

    fn schema_names(c: &Catalog) -> Vec<&str> {
        c.schemas.iter().map(|s| s.name.as_str()).collect()
    }

    fn table_names<'a>(c: &'a Catalog, schema: &str) -> Vec<&'a str> {
        c.schema(schema).unwrap().tables.iter().map(|t| t.name.as_str()).collect()
    }

    #[tokio::test]
    async fn session_applies_the_row_limit() {
        let session = engine().open_session(&cfg("fake")).await.unwrap();
        assert_eq!(session.name(), "t");
        let rs = session.query("select", 3).await.unwrap();
        assert_eq!((rs.rows.len(), rs.truncated), (3, true));
        let rs = session.query("select", 10).await.unwrap();
        assert_eq!((rs.rows.len(), rs.truncated), (5, false));
    }

    #[tokio::test]
    async fn session_setup_runs_before_the_query_and_failures_stop_execution() {
        let (engine, log) = engine_with_log();
        let session = engine.open_session(&cfg("fake")).await.unwrap();
        let rows = session.query_with_setup("USE `app`", "show tables", 3).await.unwrap();
        assert_eq!(rows.rows.len(), 3);
        assert_eq!(log.statements(), ["USE `app`", "show tables"]);
        assert!(session.query_with_setup("boom", "must not run", 3).await.is_err());
        assert_eq!(log.statements(), ["USE `app`", "show tables", "boom"]);
    }

    #[tokio::test]
    async fn execute_all_commits_together_and_rolls_back_on_failure_or_rejection() {
        let (engine, log) = engine_with_log();
        let session = engine.open_session(&cfg("fake")).await.unwrap();
        let tx = Some(["BEGIN", "COMMIT", "ROLLBACK"]);
        let statements = ["a".to_owned(), "b".to_owned()];
        session.execute_all(tx, &statements, |_, _| Ok(())).await.unwrap();
        assert_eq!(log.statements(), ["BEGIN", "a", "b", "COMMIT"]);

        let failing = ["a".to_owned(), "boom".to_owned(), "never".to_owned()];
        assert!(session.execute_all(tx, &failing, |_, _| Ok(())).await.is_err());
        assert_eq!(log.statements()[4..], ["BEGIN", "a", "boom", "ROLLBACK"]);

        let err = session
            .execute_all(tx, &statements, |i, _| {
                if i == 0 { Err("matched 2 rows".into()) } else { Ok(()) }
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("matched 2 rows"));
        assert_eq!(log.statements()[8..], ["BEGIN", "a", "ROLLBACK"]);

        session.execute_all(None, &statements, |_, _| Ok(())).await.unwrap();
        assert_eq!(log.statements()[11..], ["a", "b"]);
    }

    #[tokio::test]
    async fn reconnect_opens_the_same_source_again() {
        let (engine, log) = engine_with_log();
        let session = engine.open_session(&cfg("fake")).await.unwrap();
        session.reconnect(&engine).await.unwrap();
        let connects = log.connects.lock().unwrap();
        assert_eq!(connects.len(), 2);
        assert_eq!(connects[1].name, "t");
    }

    #[test]
    fn lost_connections_are_told_apart_from_bad_statements() {
        let lost = |m: &str| is_connection_lost(&rowcraft_dialect::Error::Query(m.into()).into());
        assert!(lost("Connection to the server is closed."));
        assert!(lost(
            "ERROR 4031 (HY000): The client was disconnected by the server because of inactivity."
        ));
        assert!(lost("connection closed"));
        assert!(lost("Input/output error: Broken pipe (os error 32)"));
        assert!(!lost("ERROR 1064 (42000): You have an error in your SQL syntax"));
        assert!(!lost("relation \"missing\" does not exist"));
    }

    #[tokio::test]
    async fn errors_keep_their_kind() {
        let session = engine().open_session(&cfg("fake")).await.unwrap();
        assert!(matches!(
            session.query("boom", 1).await,
            Err(Error::Dialect(rowcraft_dialect::Error::Query(_)))
        ));
        assert!(matches!(
            engine().open_session(&cfg("nope")).await,
            Err(Error::Dialect(rowcraft_dialect::Error::UnknownDialect(_)))
        ));
    }

    #[tokio::test]
    async fn introspect_goes_through_the_dialect_with_the_scope() {
        let all = engine().introspect(&cfg("fake"), &IntrospectScope::default()).await.unwrap();
        assert_eq!(schema_names(&all), ["public", "audit"]);
        let scope = IntrospectScope { schemas: Some(vec!["a".into(), "b".into()]) };
        let some = engine().introspect(&cfg("fake"), &scope).await.unwrap();
        assert_eq!(some.schemas.len(), 2);
    }

    #[tokio::test]
    async fn introspect_defaults_to_not_implemented() {
        let err = engine().introspect(&cfg("bare"), &IntrospectScope::default()).await;
        assert!(matches!(
            err,
            Err(Error::Dialect(rowcraft_dialect::Error::NotImplemented("introspection")))
        ));
    }

    #[tokio::test]
    async fn the_url_overrides_host_port_database_and_user_before_connecting() {
        let (engine, log) = engine_with_log();
        let mut ds = cfg("fake");
        ds.host = Some("old".into());
        ds.user = Some("me".into());
        ds.url = Some("jdbc:fake://new:7000/db".into());
        engine.connect(&ds).await.unwrap();
        let seen = log.last_connect();
        assert_eq!(
            (seen.host.as_deref(), seen.port, seen.database.as_deref(), seen.user.as_deref()),
            (Some("new"), Some(7000), Some("db"), Some("me"))
        );

        ds.url = Some("not a url".into());
        assert!(matches!(
            engine.connect(&ds).await,
            Err(Error::Config(rowcraft_config::Error::InvalidUrl(_)))
        ));
    }

    #[tokio::test]
    async fn no_auth_sends_no_password_and_windows_credentials_are_refused() {
        let (engine, log) = engine_with_log();
        let mut ds = cfg("fake");
        ds.password = Some("pre-resolved".into());
        ds.password_env = Some("ROWCRAFT_TEST_SURELY_UNSET_VAR".into());
        ds.auth = AuthType::NoAuth;
        engine.connect(&ds).await.expect("an unset password variable is ignored without auth");
        let seen = log.last_connect();
        assert_eq!((seen.password, seen.password_env), (None, None));

        ds.auth = AuthType::WindowsCredentials;
        assert!(matches!(
            engine.connect(&ds).await,
            Err(Error::Dialect(rowcraft_dialect::Error::NotImplemented("Windows credentials")))
        ));
    }

    #[tokio::test]
    async fn the_startup_script_runs_once_right_after_connecting() {
        let (engine, log) = engine_with_log();
        let mut ds = cfg("fake");
        ds.options = ConnectionOptions {
            startup_script: Some("set search_path = a;".into()),
            ..Default::default()
        };
        let session = engine.open_session(&ds).await.unwrap();
        assert_eq!(log.statements(), ["set search_path = a;"]);
        session.query("select", 1).await.unwrap();
        assert_eq!(log.statements().len(), 2, "it must not run again for later statements");

        // Blank scripts are not sent at all.
        let (engine, log) = engine_with_log();
        ds.options.startup_script = Some("  \n ".into());
        engine.connect(&ds).await.unwrap();
        assert!(log.statements().is_empty());
    }

    #[tokio::test]
    async fn a_failing_startup_script_names_itself() {
        let mut ds = cfg("fake");
        ds.options.startup_script = Some("bad script".into());
        let err = engine().connect(&ds).await.unwrap_err().to_string();
        assert!(err.contains("startup script failed") && err.contains("bad script"), "{err}");
    }

    #[tokio::test]
    async fn the_schema_pattern_filters_introspected_schemas() {
        let mut ds = cfg("fake");
        let engine = engine();
        let scope = IntrospectScope::default();

        ds.schema_pattern = Some("@:@".into()); // the default schema only
        let c = engine.introspect(&ds, &scope).await.unwrap();
        assert_eq!(schema_names(&c), ["public"]);

        ds.schema_pattern = Some("*:audit".into());
        let c = engine.introspect(&ds, &scope).await.unwrap();
        assert_eq!(schema_names(&c), ["audit"]);

        ds.schema_pattern = Some("other:*".into()); // another database: nothing matches
        let c = engine.introspect(&ds, &scope).await.unwrap();
        assert!(c.schemas.is_empty());

        ds.schema_pattern = None;
        assert_eq!(engine.introspect(&ds, &scope).await.unwrap().schemas.len(), 2);
    }

    #[tokio::test]
    async fn the_object_filter_hides_tables_sequences_and_routines() {
        let mut ds = cfg("fake");
        ds.object_filter = Some("table:-tmp_.*||routine:-tmp_.*||sequence:-orders_.*".into());
        let c = engine().introspect(&ds, &IntrospectScope::default()).await.unwrap();
        assert_eq!(table_names(&c, "public"), ["orders", "v_orders"]);
        let public = c.schema("public").unwrap();
        assert!(public.routines.is_empty() && public.sequences.is_empty());
        assert_eq!(table_names(&c, "audit"), ["log"]);

        // A rule for one kind leaves the others alone.
        ds.object_filter = Some("view:v_.*".into());
        let c = engine().introspect(&ds, &IntrospectScope::default()).await.unwrap();
        assert_eq!(table_names(&c, "public"), ["orders", "tmp_scratch", "v_orders"]);
    }

    #[tokio::test]
    async fn a_malformed_schema_setting_fails_before_connecting() {
        let (engine, log) = engine_with_log();
        let mut ds = cfg("fake");
        ds.schema_pattern = Some("*:{(}".into());
        assert!(matches!(
            engine.open_session(&ds).await,
            Err(Error::Config(rowcraft_config::Error::InvalidSetting {
                field: "schema pattern",
                ..
            }))
        ));
        assert!(log.connects.lock().unwrap().is_empty(), "no connection should have been opened");
    }

    // ---- SSH tunnels ----------------------------------------------------------------------

    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicBool, Ordering};

    use rowcraft_config::SshFile;
    use rowcraft_ssh::{Credentials, Target, TunnelOpener, Tunnels};

    /// Sets a flag when dropped, so a test can see when the tunnels are released.
    struct Released(Arc<AtomicBool>);

    impl Drop for Released {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// What the fake opener was asked: the SSH configuration, the targets, the forced port.
    type OpenCall = (SshConfig, Vec<Target>, Option<u16>);

    #[derive(Default)]
    struct FakeOpener {
        endpoints: Vec<SocketAddr>,
        fail_with_busy_port: Option<u16>,
        calls: StdMutex<Vec<OpenCall>>,
        released: Arc<AtomicBool>,
    }

    #[async_trait]
    impl TunnelOpener for FakeOpener {
        async fn open(
            &self,
            config: &SshConfig,
            _: &dyn Credentials,
            targets: &[Target],
            forced: Option<u16>,
        ) -> rowcraft_ssh::Result<Tunnels> {
            self.calls.lock().unwrap().push((config.clone(), targets.to_vec(), forced));
            if let Some(port) = self.fail_with_busy_port {
                return Err(rowcraft_ssh::Error::ForcedPortOccupied(port));
            }
            Ok(Tunnels::from_parts(
                self.endpoints.clone(),
                Box::new(Released(self.released.clone())),
            ))
        }
    }

    fn addr(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    /// An engine whose SSH configurations come from a file naming `jump`, with `opener` as the
    /// tunnel opener.
    fn engine_with_tunnels(tag: &str, opener: &Arc<FakeOpener>) -> (Engine, Arc<Log>, PathBuf) {
        let path = std::env::temp_dir()
            .join(format!("rowcraft-core-ssh-{}-{tag}", std::process::id()))
            .join("ssh.toml");
        let jump = SshConfig {
            name: "jump".into(),
            host: "bastion".into(),
            user: "me".into(),
            ..Default::default()
        };
        SshFile { configs: vec![jump] }.save(&path).unwrap();
        let (engine, log) = engine_with_log();
        let opener: Arc<dyn TunnelOpener> = opener.clone();
        (engine.with_tunnel_opener(opener).with_ssh_file(path.clone()), log, path)
    }

    fn tunneled(host: &str, port: Option<u16>) -> DataSourceConfig {
        DataSourceConfig {
            host: Some(host.into()),
            port,
            ssh_tunnel: Some("jump".into()),
            ..cfg("fake")
        }
    }

    #[tokio::test]
    async fn a_tunneled_data_source_connects_to_the_local_end() {
        let opener = Arc::new(FakeOpener { endpoints: vec![addr(41000)], ..Default::default() });
        let (engine, log, path) = engine_with_tunnels("single", &opener);
        let mut ds = tunneled("db.internal", Some(5432));
        ds.ssh_local_port = Some(41000);
        engine.connect(&ds).await.unwrap();

        let calls = opener.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1);
        let (ssh, targets, forced) = &calls[0];
        assert_eq!(ssh.name, "jump");
        assert_eq!(targets, &[Target { host: "db.internal".into(), port: 5432 }]);
        assert_eq!(*forced, Some(41000));
        let seen = log.last_connect();
        assert_eq!((seen.host.as_deref(), seen.port), (Some("127.0.0.1"), Some(41000)));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn a_cluster_connects_through_one_tunnel_per_node() {
        let opener = Arc::new(FakeOpener {
            endpoints: vec![addr(41001), addr(41002)],
            ..Default::default()
        });
        let (engine, log, path) = engine_with_tunnels("cluster", &opener);
        engine.connect(&tunneled("a:27017,b:27018", None)).await.unwrap();

        let calls = opener.calls.lock().unwrap().clone();
        assert_eq!(
            calls[0].1,
            [Target { host: "a".into(), port: 27017 }, Target { host: "b".into(), port: 27018 }]
        );
        let seen = log.last_connect();
        assert_eq!(seen.host.as_deref(), Some("127.0.0.1:41001,127.0.0.1:41002"));
        assert_eq!(seen.port, None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn the_url_decides_what_is_tunneled() {
        let opener = Arc::new(FakeOpener { endpoints: vec![addr(41003)], ..Default::default() });
        let (engine, _, path) = engine_with_tunnels("url", &opener);
        let mut ds = tunneled("ignored", Some(1));
        ds.url = Some("fake://dbhost:9999/db".into());
        engine.connect(&ds).await.unwrap();
        let calls = opener.calls.lock().unwrap().clone();
        assert_eq!(calls[0].1, [Target { host: "dbhost".into(), port: 9999 }]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn jdbc_urls_follow_the_ssh_tunnel() {
        let opener = Arc::new(FakeOpener { endpoints: vec![addr(41009)], ..Default::default() });
        let (engine, log, path) = engine_with_tunnels("jdbc-url", &opener);
        let mut ds = tunneled("ignored", Some(1));
        ds.url = Some("jdbc:fake://user@db.internal:7777/app?mode=test".into());
        engine.connect(&ds).await.unwrap();
        assert_eq!(
            log.last_connect().url.as_deref(),
            Some("jdbc:fake://user@127.0.0.1:41009/app?mode=test")
        );
        assert_eq!(
            opener.calls.lock().unwrap()[0].1,
            [Target { host: "db.internal".into(), port: 7777 }]
        );
        ds.url = Some("jdbc:oracle:thin:@db.internal:1521/app".into());
        assert!(engine.connect(&ds).await.unwrap_err().to_string().contains("opaque JDBC URL"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn the_tunnel_lives_exactly_as_long_as_the_connection() {
        let opener = Arc::new(FakeOpener { endpoints: vec![addr(41004)], ..Default::default() });
        let (engine, _, path) = engine_with_tunnels("lifetime", &opener);
        let session = engine.open_session(&tunneled("db", Some(1))).await.unwrap();
        assert!(!opener.released.load(Ordering::SeqCst), "released while the session is alive");
        session.query("select", 1).await.unwrap(); // the wrapper must still pass queries through
        drop(session);
        assert!(
            opener.released.load(Ordering::SeqCst),
            "not released after the session was dropped"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn without_a_tunnel_the_opener_is_not_used() {
        let opener = Arc::new(FakeOpener::default());
        let (engine, log, path) = engine_with_tunnels("none", &opener);
        let mut ds = cfg("fake");
        ds.host = Some("direct".into());
        engine.connect(&ds).await.unwrap();
        assert!(opener.calls.lock().unwrap().is_empty());
        assert_eq!(log.last_connect().host.as_deref(), Some("direct"));
        ds.ssh_tunnel = Some("   ".into()); // blank means none
        engine.connect(&ds).await.unwrap();
        assert!(opener.calls.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn tunnel_problems_stop_the_connection_before_the_database_is_touched() {
        let busy = Arc::new(FakeOpener { fail_with_busy_port: Some(41005), ..Default::default() });
        let (engine, log, path) = engine_with_tunnels("busy", &busy);
        let err = engine.connect(&tunneled("db", Some(1))).await.unwrap_err();
        assert!(matches!(err, Error::Ssh(rowcraft_ssh::Error::ForcedPortOccupied(41005))), "{err}");
        assert!(log.connects.lock().unwrap().is_empty());

        // A name that is not in ssh.toml.
        let mut ds = tunneled("db", Some(1));
        ds.ssh_tunnel = Some("nope".into());
        let err = engine.connect(&ds).await.unwrap_err();
        assert!(
            matches!(&err, Error::Config(rowcraft_config::Error::SshNotFound(m)) if m == "nope"),
            "{err}"
        );

        // No port anywhere: the message says so rather than guessing.
        let err = engine.connect(&tunneled("db", None)).await.unwrap_err();
        assert!(
            matches!(&err, Error::Ssh(rowcraft_ssh::Error::Target(m)) if m.contains("no port")),
            "{err}"
        );
        assert!(busy.calls.lock().unwrap().len() == 1, "only the first attempt reached the opener");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn the_dialects_default_port_is_used_when_none_is_given() {
        struct WithPort(Arc<Log>);
        #[async_trait]
        impl Dialect for WithPort {
            fn id(&self) -> DialectId {
                DialectId("ported")
            }
            fn display_name(&self) -> &'static str {
                "Ported"
            }
            fn family(&self) -> DialectFamily {
                DialectFamily::Sql
            }
            fn default_port(&self) -> Option<u16> {
                Some(5432)
            }
            async fn connect(
                &self,
                cfg: &DataSourceConfig,
            ) -> rowcraft_dialect::Result<Box<dyn Connection>> {
                self.0.connects.lock().unwrap().push(cfg.clone());
                Ok(Box::new(FakeConnection(self.0.clone())))
            }
        }
        let log = Arc::new(Log::default());
        let mut registry = Registry::new();
        registry.register(WithPort(log));
        let opener = Arc::new(FakeOpener { endpoints: vec![addr(41006)], ..Default::default() });
        let path = std::env::temp_dir()
            .join(format!("rowcraft-core-ssh-{}-port", std::process::id()))
            .join("ssh.toml");
        SshFile {
            configs: vec![SshConfig {
                name: "jump".into(),
                host: "bastion".into(),
                user: "me".into(),
                ..Default::default()
            }],
        }
        .save(&path)
        .unwrap();
        let opener_dyn: Arc<dyn TunnelOpener> = opener.clone();
        let engine =
            Engine::new(registry).with_tunnel_opener(opener_dyn).with_ssh_file(path.clone());
        let ds = DataSourceConfig {
            host: Some("db".into()),
            ssh_tunnel: Some("jump".into()),
            ..cfg("ported")
        };
        engine.connect(&ds).await.unwrap();
        assert_eq!(opener.calls.lock().unwrap()[0].1, [Target { host: "db".into(), port: 5432 }]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn an_unsaved_ssh_configuration_can_stand_in_for_the_saved_one() {
        let opener = Arc::new(FakeOpener { endpoints: vec![addr(41007)], ..Default::default() });
        let (engine, _, path) = engine_with_tunnels("unsaved", &opener);
        let mut ds = tunneled("db", Some(1));
        ds.ssh_tunnel = Some("being-edited".into());
        // Not in ssh.toml, so a plain connect fails...
        assert!(engine.connect(&ds).await.is_err());
        // ...but the form can pass the configuration it holds.
        let draft = SshConfig {
            name: "being-edited".into(),
            host: "new-bastion".into(),
            user: "u".into(),
            ..Default::default()
        };
        engine.connect_with(&ds, Some(&draft)).await.unwrap();
        assert_eq!(opener.calls.lock().unwrap()[0].0.host, "new-bastion");
        // A stand-in for some other name is ignored.
        let other = SshConfig { name: "other".into(), ..draft };
        assert!(engine.connect_with(&ds, Some(&other)).await.is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
