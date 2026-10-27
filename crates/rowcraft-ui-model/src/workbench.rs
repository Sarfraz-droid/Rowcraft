//! The state behind the main window: the data source list, the open session, and the operations
//! the UI triggers. Methods return plain serializable values and `String` errors meant for the
//! status bar, so the desktop shell can hand them straight to the frontend.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use rowcraft_config::{
    CustomDriverConfig, DataSourceConfig, DataSourceFile, DriverFile, SshFile, datasources_path,
    ssh_path,
};
use rowcraft_core::{Engine, Session};
use rowcraft_dialect::IntrospectScope;
use rowcraft_model::Catalog;
use serde::{Deserialize, Serialize};

use crate::result::{QueryOutput, ROW_LIMIT};
use crate::source_form::{self, Draft};

/// Where the configuration lives. Tests point these at a temporary directory.
#[derive(Debug, Clone)]
pub struct Paths {
    pub datasources: PathBuf,
    pub ssh: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self { datasources: datasources_path(), ssh: ssh_path() }
    }
}

/// One row of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceSummary {
    pub name: String,
    pub dialect: String,
    pub host: Option<String>,
    pub ssh_tunnel: Option<String>,
    /// The data source's color (`#rrggbb`), if it has one.
    pub color: Option<String>,
}

impl From<&DataSourceConfig> for SourceSummary {
    fn from(c: &DataSourceConfig) -> Self {
        Self {
            name: c.name.clone(),
            dialect: c.dialect.clone(),
            host: c.host.clone(),
            ssh_tunnel: c.ssh_tunnel.clone(),
            color: c.color.clone(),
        }
    }
}

/// What the window needs when it opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bootstrap {
    pub sources: Vec<SourceSummary>,
    /// A problem found while reading the configuration, to show in the status bar.
    pub startup_error: Option<String>,
}

/// What the "Add data source" dialog needs when it opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FormDefaults {
    pub draft: Draft,
    /// Saved SSH configurations a data source may use.
    pub ssh_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceEditDefaults {
    pub draft: Draft,
    pub ssh_names: Vec<String>,
    pub has_saved_password: bool,
    pub has_saved_key_password: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AddedSource {
    pub sources: Vec<SourceSummary>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DialectInfo {
    pub id: &'static str,
    pub name: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriverSettings {
    pub custom: Vec<CustomDriverConfig>,
    pub dialects: Vec<DialectInfo>,
}

#[derive(Debug, Clone)]
struct RetainedSession {
    session: Arc<Session>,
    dialect: String,
    catalog: Option<Catalog>,
}

#[derive(Debug, Default)]
struct State {
    sources: Vec<DataSourceConfig>,
    sessions: HashMap<String, RetainedSession>,
    source_revision: u64,
    session: Option<Arc<Session>>,
    /// Dialect id of the data source behind `session`; decides identifier quoting.
    dialect: Option<String>,
    catalog: Option<Catalog>,
}

pub struct Workbench {
    engine: Arc<Engine>,
    paths: Paths,
    startup_error: Option<String>,
    state: Mutex<State>,
    driver_lock: Mutex<()>,
    source_lock: Mutex<()>,
    connection_lock: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for Workbench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workbench").field("paths", &self.paths).finish_non_exhaustive()
    }
}

impl Workbench {
    /// Reads the data sources. A broken file or `drivers.toml` entry does not stop the app: it
    /// becomes `startup_error` and the list is empty. `driver_warnings` are the messages from
    /// `rowcraft_config::register_custom_drivers`, which the caller runs before anything reads the catalog.
    pub fn open(engine: Arc<Engine>, paths: Paths, driver_warnings: Vec<String>) -> Self {
        let (sources, error) = match DataSourceFile::load(&paths.datasources) {
            Ok(file) => (file.datasources, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        let startup_error = match (error, driver_warnings.is_empty()) {
            (error, true) => error,
            (error, false) => {
                let drivers = format!("drivers.toml: {}", driver_warnings.join("; "));
                Some(error.map_or_else(|| drivers.clone(), |e| format!("{e}\n{drivers}")))
            }
        };
        Self {
            engine,
            paths,
            startup_error,
            state: Mutex::new(State { sources, ..State::default() }),
            driver_lock: Mutex::new(()),
            source_lock: Mutex::new(()),
            connection_lock: tokio::sync::Mutex::new(()),
        }
    }

    // A panic while holding the lock cannot leave `State` half-updated (every update is one
    // assignment), so a poisoned lock is still good to use.
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn bootstrap(&self) -> Bootstrap {
        Bootstrap {
            sources: self.state().sources.iter().map(SourceSummary::from).collect(),
            startup_error: self.startup_error.clone(),
        }
    }

    pub fn source_properties(
        &self,
        name: &str,
    ) -> Result<crate::source_properties::SourceProperties, String> {
        let state = self.state();
        let source = state
            .sources
            .iter()
            .find(|source| source.name == name)
            .ok_or_else(|| format!("No data source named `{name}`."))?;
        crate::source_properties::SourceProperties::from_config(source)
    }

    /// Dialects that are part of this build.
    fn known_dialects(&self) -> Vec<&'static str> {
        self.engine.registry().iter().map(|d| d.id().0).collect()
    }

    /// Dialects whose `connect` works today.
    pub fn ready_dialects(&self) -> Vec<&'static str> {
        self.engine.registry().iter().filter(|d| d.connects()).map(|d| d.id().0).collect()
    }

    pub fn drivers(&self, query: &str) -> Vec<crate::DriverInfo> {
        crate::list_drivers(query, &self.ready_dialects())
    }

    fn drivers_path(&self) -> PathBuf {
        self.paths.datasources.with_file_name("drivers.toml")
    }

    pub fn driver_settings(&self) -> Result<DriverSettings, String> {
        let file = DriverFile::load(&self.drivers_path()).map_err(|e| e.to_string())?;
        Ok(DriverSettings {
            custom: file.drivers,
            dialects: self
                .engine
                .registry()
                .iter()
                .map(|d| DialectInfo { id: d.id().0, name: d.display_name() })
                .collect(),
        })
    }

    pub fn save_driver(
        &self,
        driver: CustomDriverConfig,
    ) -> Result<Vec<crate::DriverInfo>, String> {
        let _guard = self.driver_lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        driver.validate()?;
        self.engine.registry().get(&driver.dialect).map_err(|e| e.to_string())?;
        let path = self.drivers_path();
        let mut file = DriverFile::load(&path).map_err(|e| e.to_string())?;
        if rowcraft_drivers::find(&driver.id).is_some_and(|d| !d.custom) {
            return Err("Built-in driver IDs cannot be replaced. Choose a new ID.".into());
        }
        if let Some(existing) = file.drivers.iter_mut().find(|d| d.id == driver.id) {
            if (existing.dialect != driver.dialect || existing.target != driver.target)
                && DataSourceFile::load(&self.paths.datasources)
                    .map_err(|e| e.to_string())?
                    .datasources
                    .iter()
                    .any(|s| s.driver.as_deref() == Some(&driver.id))
            {
                return Err("This driver's protocol or target cannot change while a saved data source uses it.".into());
            }
            *existing = driver;
        } else {
            file.add(driver).map_err(|e| e.to_string())?;
        }
        file.save(&path).map_err(|e| e.to_string())?;
        file.register();
        Ok(self.drivers(""))
    }

    pub fn remove_driver(&self, id: &str) -> Result<Vec<crate::DriverInfo>, String> {
        let _guard = self.driver_lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let sources = DataSourceFile::load(&self.paths.datasources).map_err(|e| e.to_string())?;
        if sources.datasources.iter().any(|s| s.driver.as_deref() == Some(id)) {
            return Err(
                "This driver is used by a saved data source. Remove that data source first.".into(),
            );
        }
        let path = self.drivers_path();
        let mut file = DriverFile::load(&path).map_err(|e| e.to_string())?;
        file.remove(id).map_err(|e| e.to_string())?;
        file.save(&path).map_err(|e| e.to_string())?;
        file.register();
        Ok(self.drivers(""))
    }

    fn ssh_names(&self) -> Vec<String> {
        SshFile::load(&self.paths.ssh)
            .map(|f| f.configs.into_iter().map(|c| c.name).collect())
            .unwrap_or_default()
    }

    pub fn form_defaults(&self) -> FormDefaults {
        FormDefaults { draft: Draft::initial(), ssh_names: self.ssh_names() }
    }

    pub fn edit_source(&self, name: &str) -> Result<SourceEditDefaults, String> {
        let file = DataSourceFile::load(&self.paths.datasources).map_err(|e| e.to_string())?;
        let config = file.get(name).map_err(|e| e.to_string())?;
        Ok(SourceEditDefaults {
            draft: Draft::from_config(config),
            ssh_names: self.ssh_names(),
            has_saved_password: config.password_keychain.is_some() || config.password_env.is_some(),
            has_saved_key_password: config.ssl_files.key_password.account.is_some()
                || config.ssl_files.key_password_env.is_some(),
        })
    }

    pub fn update_source(&self, original_name: &str, draft: &Draft) -> Result<AddedSource, String> {
        let _guard = self.source_lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut file = DataSourceFile::load(&self.paths.datasources).map_err(|e| e.to_string())?;
        let original = file.get(original_name).map_err(|e| e.to_string())?.clone();
        let others: Vec<_> =
            file.datasources.iter().filter(|c| c.name != original_name).cloned().collect();
        let valid = draft.validate_full(&self.known_dialects(), &others, &self.ssh_names())?;
        let mut config = valid.config;
        draft.preserve_saved_secrets(&original, &mut config);
        let name = config.name.clone();
        let mut pending = rowcraft_config::passwords::PendingSecrets::default();
        if let Some(password) = config.password.as_deref() {
            config.password_keychain = Some(pending.store(&format!("{name}:database"), password)?);
        }
        pending.store_secret(&format!("{name}:ssl-key"), &mut config.ssl_files.key_password)?;
        if let Some(ssh) = valid.new_ssh {
            self.persist_ssh_config(ssh)?;
        }
        let index = file
            .datasources
            .iter()
            .position(|c| c.name == original_name)
            .ok_or("The data source was removed.")?;
        file.datasources[index] = config;
        // Replace the file atomically so a failed save cannot truncate the existing configuration.
        let temporary =
            self.paths.datasources.with_extension(format!("toml.{}.tmp", std::process::id()));
        let saved = file.save(&temporary).map_err(|e| e.to_string()).and_then(|_| {
            std::fs::rename(&temporary, &self.paths.datasources).map_err(|e| e.to_string())
        });
        if let Err(error) = saved {
            let _ = std::fs::remove_file(temporary);
            return Err(error);
        }
        pending.commit();
        let sources = file.datasources.iter().map(SourceSummary::from).collect();
        {
            let mut state = self.state();
            state.sources = file.datasources.clone();
            state.sessions.remove(original_name);
            state.source_revision = state.source_revision.wrapping_add(1);
            if state.session.as_ref().is_some_and(|s| s.name() == original_name) {
                state.session = None;
                state.dialect = None;
                state.catalog = None;
            }
        }
        for account in [original.password_keychain, original.ssl_files.key_password.account]
            .into_iter()
            .flatten()
        {
            if !file.datasources.iter().any(|c| {
                c.password_keychain.as_ref() == Some(&account)
                    || c.ssl_files.key_password.account.as_ref() == Some(&account)
            }) {
                let _ = rowcraft_config::passwords::delete(&account);
            }
        }
        Ok(AddedSource { sources, name })
    }

    pub fn sqlite_import_directory(&self) -> PathBuf {
        self.paths.datasources.with_file_name("sqlite-imports")
    }

    pub fn jdbc_import_directory(&self) -> PathBuf {
        self.paths.datasources.with_file_name("jdbc-imports")
    }

    /// Saves a reusable SSH configuration independently of any database connection.
    pub fn save_ssh_config(&self, draft: &Draft) -> Result<Vec<String>, String> {
        self.persist_ssh_config(draft.new_ssh_config()?)?;
        Ok(self.ssh_names())
    }

    fn persist_ssh_config(&self, mut ssh: rowcraft_config::SshConfig) -> Result<(), String> {
        if self.ssh_names().contains(&ssh.name) {
            return Err(format!("An SSH configuration named `{}` already exists.", ssh.name));
        }
        let mut pending = rowcraft_config::passwords::PendingSecrets::default();
        pending.store_secret(&format!("{}:ssh-password", ssh.name), &mut ssh.password)?;
        pending.store_secret(&format!("{}:ssh-passphrase", ssh.name), &mut ssh.passphrase)?;
        source_form::save_new_ssh(&self.paths.ssh, ssh)?;
        pending.commit();
        Ok(())
    }

    /// Validates `draft` and appends it to `datasources.toml` (and a new SSH configuration to
    /// `ssh.toml` first, since the data source names it). On a problem nothing is saved.
    pub fn add_source(&self, draft: &Draft) -> Result<AddedSource, String> {
        let _guard = self.source_lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let existing = self.state().sources.clone();
        let ssh_names = self.ssh_names();
        let valid = draft.validate_full(&self.known_dialects(), &existing, &ssh_names)?;
        let name = valid.config.name.clone();
        let mut config = valid.config;
        let mut pending = rowcraft_config::passwords::PendingSecrets::default();
        if let Some(password) = config.password.as_deref() {
            config.password_keychain = Some(pending.store(&format!("{name}:database"), password)?);
        }
        pending.store_secret(&format!("{name}:ssl-key"), &mut config.ssl_files.key_password)?;
        if let Some(ssh) = valid.new_ssh {
            self.persist_ssh_config(ssh)?;
        }
        let all = source_form::save_new(&self.paths.datasources, config)?;
        pending.commit();
        let sources = all.iter().map(SourceSummary::from).collect();
        self.state().sources = all;
        Ok(AddedSource { sources, name })
    }

    /// "Test Connection": connects with the form's settings, then drops the connection.
    pub async fn test_connection(&self, draft: &Draft) -> Result<(), String> {
        self.test_source_connection(draft, None).await
    }

    pub async fn test_source_connection(
        &self,
        draft: &Draft,
        original_name: Option<&str>,
    ) -> Result<(), String> {
        let mut draft = draft.clone();
        if draft.name.trim().is_empty() {
            draft.name = "test".into(); // a name is not needed to try a connection
        }
        let mut valid = draft.validate_full(&self.known_dialects(), &[], &self.ssh_names())?;
        if let Some(name) = original_name {
            let file = DataSourceFile::load(&self.paths.datasources).map_err(|e| e.to_string())?;
            draft.preserve_saved_secrets(
                file.get(name).map_err(|e| e.to_string())?,
                &mut valid.config,
            );
        }
        self.engine
            .connect_with(&valid.config, valid.new_ssh.as_ref())
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Selects a retained session, opening it only on the first use of this source.
    pub async fn connect(&self, name: &str) -> Result<String, String> {
        self.select_connection(name, false).await
    }

    /// Explicitly replaces this source's session without closing other sources.
    pub async fn reconnect(&self, name: &str) -> Result<String, String> {
        self.select_connection(name, true).await
    }

    async fn select_connection(&self, name: &str, reconnect: bool) -> Result<String, String> {
        // Serialize opens so repeated requests cannot create duplicate sessions.
        let _guard = self.connection_lock.lock().await;
        let (cfg, revision) = {
            let mut state = self.state();
            let cfg = state
                .sources
                .iter()
                .find(|source| source.name == name)
                .cloned()
                .ok_or_else(|| format!("No data source named `{name}`."))?;
            if reconnect {
                state.sessions.remove(name);
            }
            if let Some(retained) = state.sessions.get(name).cloned() {
                state.session = Some(retained.session);
                state.dialect = Some(retained.dialect);
                state.catalog = retained.catalog;
                return Ok(name.to_owned());
            }
            state.session = None;
            state.dialect = None;
            state.catalog = None;
            (cfg, state.source_revision)
        };
        let session = self.engine.open_session(&cfg).await.map_err(|error| error.to_string())?;
        let mut state = self.state();
        if state.source_revision != revision || !state.sources.iter().any(|source| source == &cfg) {
            return Err("The data source changed while connecting. Connect again to use its current settings.".into());
        }
        state.sessions.insert(
            name.to_owned(),
            RetainedSession {
                session: session.clone(),
                dialect: cfg.dialect.clone(),
                catalog: None,
            },
        );
        state.session = Some(session);
        state.dialect = Some(cfg.dialect);
        Ok(name.to_owned())
    }

    /// Removes the data source `name` from `datasources.toml`. If it is the connected one, the
    /// session is closed too.
    pub fn remove_source(&self, name: &str) -> Result<Vec<SourceSummary>, String> {
        let _guard = self.source_lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut file = DataSourceFile::load(&self.paths.datasources).map_err(|e| e.to_string())?;
        let removed = file.get(name).map_err(|e| e.to_string())?;
        let accounts: Vec<String> =
            [removed.password_keychain.clone(), removed.ssl_files.key_password.account.clone()]
                .into_iter()
                .flatten()
                .collect();
        file.remove(name).map_err(|e| e.to_string())?;
        file.save(&self.paths.datasources).map_err(|e| e.to_string())?;
        let summaries = file.datasources.iter().map(SourceSummary::from).collect();
        let mut state = self.state();
        state.sessions.remove(name);
        state.source_revision = state.source_revision.wrapping_add(1);
        if state.session.as_ref().is_some_and(|s| s.name() == name) {
            state.session = None;
            state.dialect = None;
            state.catalog = None;
        }
        state.sources = file.datasources;
        drop(state);
        for account in accounts {
            let _ = rowcraft_config::passwords::delete(&account);
        }
        Ok(summaries)
    }

    /// Reads the schemas, tables and columns of the connected data source for the explorer.
    pub async fn introspect(&self) -> Result<Catalog, String> {
        let session =
            self.state().session.clone().ok_or_else(|| "Select a data source first.".to_owned())?;
        let scope = IntrospectScope::default();
        let catalog =
            self.retrying(&session, |s| s.introspect(&scope)).await.map_err(|e| e.to_string())?;
        let mut state = self.state();
        if let Some(retained) = state
            .sessions
            .get_mut(session.name())
            .filter(|retained| Arc::ptr_eq(&retained.session, &session))
        {
            retained.catalog = Some(catalog.clone());
        }
        if state.session.as_ref().is_some_and(|current| Arc::ptr_eq(current, &session)) {
            state.catalog = Some(catalog.clone());
        }
        Ok(catalog)
    }

    /// The statement behind "open table": the first rows of `schema.table`, quoted the way the
    /// connected database expects.
    pub fn preview_statement(&self, schema: &str, table: &str) -> Result<String, String> {
        let dialect =
            self.state().dialect.clone().ok_or_else(|| "Select a data source first.".to_owned())?;
        match dialect.as_str() {
            "mongo" => {
                return Ok(format!(
                    "db.getCollection({}).find({{}}).limit({ROW_LIMIT})",
                    serde_json::to_string(table).map_err(|e| e.to_string())?
                ));
            }
            "redis" => {
                let pattern = table.chars().fold(String::new(), |mut value, ch| {
                    if "\\*?[]".contains(ch) {
                        value.push('\\');
                    }
                    value.push(ch);
                    value
                });
                return Ok(format!(
                    "SCAN 0 MATCH {} COUNT 100",
                    serde_json::to_string(&format!("{pattern}*")).map_err(|e| e.to_string())?
                ));
            }
            "neo4j" => {
                return Ok(format!(
                    "MATCH (n:`{}`) RETURN n LIMIT {ROW_LIMIT}",
                    table.replace('`', "``")
                ));
            }
            "graphql" => {
                let state = self.state();
                let preview = state
                    .catalog
                    .as_ref()
                    .and_then(|catalog| catalog.schema(schema))
                    .and_then(|schema| schema.table(table))
                    .and_then(|table| table.comment.clone())
                    .ok_or("Refresh the GraphQL schema before previewing this query.")?;
                if !preview.starts_with("query {") {
                    return Err(preview);
                }
                return Ok(preview);
            }
            _ => {}
        }
        let (open, close) = quote_pair(&dialect);
        let ident =
            |s: &str| format!("{open}{}{close}", s.replace(close, &format!("{close}{close}")));
        let name = if dialect == "couchbase" {
            schema
                .split('.')
                .filter(|part| !part.is_empty())
                .chain(std::iter::once(table))
                .map(ident)
                .collect::<Vec<_>>()
                .join(".")
        } else if schema.is_empty() || dialect == "dynamo" {
            ident(table)
        } else {
            format!("{}.{}", ident(schema), ident(table))
        };
        Ok(format!("select * from {name}"))
    }

    /// Saves the edits made in the result grid to `schema.table`: `changes.deletes` and the rows of
    /// `changes.updates` are result rows (cells in `columns` order, as the grid received them),
    /// matched by the primary key, or by a unique key whose columns are all NOT NULL. Tables
    /// without such a key are refused rather than guessed at.
    ///
    /// Everything runs in one transaction where the database has them, and a statement that does
    /// not hit exactly the rows it was meant for rolls it all back, so a row that changed or
    /// vanished since it was read is never half-saved.
    pub async fn apply_row_changes(
        &self,
        schema: &str,
        table: &str,
        columns: &[String],
        changes: &RowChanges,
    ) -> Result<QueryOutput, String> {
        let (session, dialect, statements) = {
            let state = self.state();
            let session =
                state.session.clone().ok_or_else(|| "Select a data source first.".to_owned())?;
            let dialect = state.dialect.clone().unwrap_or_default();
            let meta = state
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.table(schema, table))
                .ok_or("Refresh the database objects before editing rows of this table.")?;
            let statements = change_statements(&dialect, schema, meta, columns, changes)?;
            (session, dialect, statements)
        };
        let sql: Vec<String> = statements.iter().map(|(sql, _)| sql.clone()).collect();
        let check = |i: usize, rs: &rowcraft_dialect::ResultSet| match (
            statements[i].1,
            rs.rows_affected,
        ) {
            (Some(expected), Some(actual)) if actual != expected => Err(format!(
                "`{}` matched {actual} row(s) instead of {expected}; the rows may have changed \
                 since they were read. Nothing was saved. Reload the rows and try again.",
                statements[i].0
            )),
            _ => Ok(()),
        };
        let started = Instant::now();
        let tx = transaction_verbs(&dialect);
        let affected = match tx {
            // A dropped connection rolls the transaction back, so it is safe to run it again.
            Some(_) => self.retrying(&session, |s| s.execute_all(tx, &sql, check)).await,
            None => session.execute_all(None, &sql, check).await,
        }
        .map_err(|e| e.to_string())?;
        Ok(QueryOutput {
            columns: Vec::new(),
            rows: Vec::new(),
            summary: format!(
                "{affected} row{} affected in {} ms",
                if affected == 1 { "" } else { "s" },
                started.elapsed().as_millis()
            ),
            row_limit: None,
        })
    }

    /// Runs `op` on `session`; when the connection turns out to be gone (the server closed it
    /// while the app sat idle, the laptop slept, the tunnel dropped), opens it again and runs `op`
    /// once more. Only a lost connection is retried, never a failed statement.
    async fn retrying<'s, T, Fut>(
        &self,
        session: &'s Session,
        op: impl Fn(&'s Session) -> Fut,
    ) -> rowcraft_core::Result<T>
    where
        Fut: std::future::Future<Output = rowcraft_core::Result<T>>,
    {
        match op(session).await {
            Err(e) if rowcraft_core::is_connection_lost(&e) => {
                session.reconnect(&self.engine).await?;
                op(session).await
            }
            other => other,
        }
    }

    fn ai_source(&self, name: &str) -> Result<DataSourceConfig, String> {
        let mut cfg = self
            .state()
            .sources
            .iter()
            .find(|source| source.name == name)
            .cloned()
            .ok_or_else(|| format!("No data source named `{name}`."))?;
        cfg.read_only = true;
        cfg.options.startup_script = None;
        Ok(cfg)
    }

    /// A separate read-only connection keeps assistant tools independent of console sessions.
    pub async fn ai_schema(&self, name: &str, schema: Option<&str>) -> Result<Catalog, String> {
        let cfg = self.ai_source(name)?;
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let mut connection = self.engine.connect(&cfg).await.map_err(|e| e.to_string())?;
            let dialect = self.engine.registry().get(&cfg.dialect).map_err(|e| e.to_string())?;
            let scope = IntrospectScope {
                schemas: schema.filter(|s| !s.is_empty()).map(|s| vec![s.to_owned()]),
            };
            dialect.introspect(connection.as_mut(), &scope).await.map_err(|e| e.to_string())
        })
        .await
        .map_err(|_| "Schema inspection timed out after 15 seconds.".to_owned())?
    }

    pub async fn ai_query(
        &self,
        name: &str,
        schema: Option<&str>,
        statement: &str,
    ) -> Result<QueryOutput, String> {
        let statement = crate::ai_query::validate(statement)?;
        let cfg = self.ai_source(name)?;
        if !["pg", "pgbase", "cockroach", "redshift", "greenplum", "mysql", "mariadb", "sqlite"]
            .contains(&cfg.dialect.as_str())
        {
            return Err("Read-only AI query tools currently support PostgreSQL, MySQL, MariaDB, and SQLite. Schema inspection is still available.".into());
        }
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let started = Instant::now();
            let mut connection = self.engine.connect(&cfg).await.map_err(|e| e.to_string())?;
            if cfg.dialect != "sqlite" {
                if let Some(schema) = schema.filter(|s| !s.is_empty()) {
                    connection
                        .query(&schema_statement(&cfg.dialect, schema)?)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            match cfg.dialect.as_str() {
                "mysql" => {
                    connection
                        .query("SET SESSION MAX_EXECUTION_TIME = 10000")
                        .await
                        .map_err(|e| e.to_string())?;
                    connection
                        .query("START TRANSACTION READ ONLY")
                        .await
                        .map_err(|e| e.to_string())?;
                }
                "mariadb" => {
                    connection
                        .query("SET SESSION max_statement_time = 10")
                        .await
                        .map_err(|e| e.to_string())?;
                    connection
                        .query("START TRANSACTION READ ONLY")
                        .await
                        .map_err(|e| e.to_string())?;
                }
                "sqlite" => {
                    connection.query("PRAGMA query_only = ON").await.map_err(|e| e.to_string())?;
                }
                _ => {
                    connection.query("BEGIN READ ONLY").await.map_err(|e| e.to_string())?;
                    connection
                        .query("SET LOCAL statement_timeout = '10s'")
                        .await
                        .map_err(|e| e.to_string())?;
                    connection
                        .query("SET LOCAL lock_timeout = '2s'")
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            let result = connection.query_limited(&statement, 25).await;
            if cfg.dialect != "sqlite" {
                let _ = connection.query("ROLLBACK").await;
            }
            let mut output =
                QueryOutput::new(result.map_err(|e| e.to_string())?, started.elapsed());
            output.summary = format!("{} rows returned (25 row limit)", output.rows.len());
            output.columns.truncate(32);
            for row in &mut output.rows {
                row.truncate(32);
                for cell in row {
                    if let serde_json::Value::String(text) = cell {
                        if text.chars().count() > 256 {
                            *text = text.chars().take(256).collect::<String>() + "…";
                        }
                    }
                }
            }
            Ok(output)
        })
        .await
        .map_err(|_| "AI query timed out after 15 seconds.".to_owned())?
    }

    /// Runs `statement` on the current session. The previous result stays on screen when this
    /// fails, so a typo does not wipe it: that is the frontend's job, an `Err` carries only the message.
    pub async fn run(&self, statement: &str) -> Result<QueryOutput, String> {
        self.run_in_schema(statement, None).await
    }

    pub async fn run_in_schema(
        &self,
        statement: &str,
        schema: Option<&str>,
    ) -> Result<QueryOutput, String> {
        self.run_with_limit(statement, schema, false).await
    }

    pub async fn run_with_limit(
        &self,
        statement: &str,
        schema: Option<&str>,
        show_all: bool,
    ) -> Result<QueryOutput, String> {
        let (session, dialect) = {
            let state = self.state();
            (
                state.session.clone().ok_or_else(|| "Select a data source first.".to_owned())?,
                state.dialect.clone().unwrap_or_default(),
            )
        };
        let bounded = if show_all {
            None
        } else {
            crate::query_preview::limited_sql(statement, &dialect, ROW_LIMIT)
        };
        let applied_limit = bounded.is_some();
        let statement = bounded.as_deref().unwrap_or(statement);
        let max_rows = if show_all { usize::MAX } else { ROW_LIMIT };
        let setup = match schema.filter(|name| !name.is_empty()) {
            Some(schema) => Some(schema_statement(&dialect, schema)?),
            None => None,
        };
        let setup = setup.as_deref();
        let started = Instant::now();
        let result = self
            .retrying(&session, |s| async move {
                match setup {
                    Some(setup) => s.query_with_setup(setup, statement, max_rows).await,
                    None => s.query(statement, max_rows).await,
                }
            })
            .await;
        let rs = result.map_err(|error| {
            let message = error.to_string();
            if message.contains("1046") || message.contains("No database selected") {
                "No database selected. Choose a schema in the console toolbar, then run the query again.".to_owned()
            } else { message }
        })?;
        let mut output = QueryOutput::new(rs, started.elapsed());
        if applied_limit && !output.columns.is_empty() {
            output.row_limit = Some(ROW_LIMIT);
            output.summary.push_str(&format!(" · Default limit: {ROW_LIMIT} rows"));
        }
        Ok(output)
    }
}

fn schema_statement(dialect: &str, schema: &str) -> Result<String, String> {
    let (open, close) = quote_pair(dialect);
    let identifier = format!("{open}{}{close}", schema.replace(close, &format!("{close}{close}")));
    match dialect {
        "mongo" => {
            if schema.chars().any(|ch| ch.is_whitespace() || "/\\.\"$*;".contains(ch)) {
                return Err("Invalid MongoDB database name.".into());
            }
            Ok(format!("use {schema}"))
        }
        "redis" => schema
            .parse::<u32>()
            .map(|db| format!("SELECT {db}"))
            .map_err(|_| "Choose a numeric Redis database.".into()),
        "cassandra" => Ok(format!("USE {identifier}")),
        "mysql" | "mariadb" | "clickhouse" => Ok(format!("USE {identifier}")),
        "pg" | "pgbase" | "cockroach" | "redshift" | "greenplum" => {
            Ok(format!("SET search_path TO {identifier}"))
        }
        _ => Err("Schema selection is not available for this database driver.".into()),
    }
}

/// The columns that identify one row: the primary key, else the first unique key whose columns
/// are all NOT NULL (NULLs never compare equal, so a nullable key cannot find its row).
fn row_key(table: &rowcraft_model::Table) -> Option<Vec<String>> {
    use rowcraft_model::ConstraintKind;
    let primary = table.primary_key();
    if !primary.is_empty() {
        return Some(primary.to_vec());
    }
    table
        .constraints
        .iter()
        .filter(|c| c.kind == ConstraintKind::Unique && !c.columns.is_empty())
        .find(|c| {
            c.columns
                .iter()
                .all(|name| table.columns.iter().any(|col| &col.name == name && !col.nullable))
        })
        .map(|c| c.columns.clone())
}

/// Edits made in the result grid, as the frontend sends them. Values are JSON cells: `null` is
/// SQL NULL, text is typed by the user and left for the database to convert.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct RowChanges {
    /// Result rows to delete.
    pub deletes: Vec<Vec<serde_json::Value>>,
    pub updates: Vec<RowUpdate>,
    /// New rows, column name to value; columns left out get their default.
    pub inserts: Vec<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RowUpdate {
    /// The result row as it was read, which locates the row by its key.
    pub row: Vec<serde_json::Value>,
    /// Column name to new value.
    pub values: serde_json::Map<String, serde_json::Value>,
}

/// Rows deleted per statement: few round trips, while keeping statements a sensible size.
const DELETE_CHUNK: usize = 200;

/// The statements that apply `changes` to `table`, deletes first (so a new row may reuse a
/// deleted key), each with the number of rows it must affect, where that is known.
fn change_statements(
    dialect: &str,
    schema: &str,
    table: &rowcraft_model::Table,
    columns: &[String],
    changes: &RowChanges,
) -> Result<Vec<(String, Option<u64>)>, String> {
    if matches!(
        dialect,
        "mongo" | "redis" | "neo4j" | "graphql" | "dynamo" | "couchbase" | "cassandra"
    ) {
        return Err("Editing rows from the grid is only available for SQL databases.".into());
    }
    if changes.deletes.is_empty() && changes.updates.is_empty() && changes.inserts.is_empty() {
        return Err("There are no changes to save.".into());
    }
    let (open, close) = quote_pair(dialect);
    let ident = |s: &str| format!("{open}{}{close}", s.replace(close, &format!("{close}{close}")));
    let name = if schema.is_empty() {
        ident(&table.name)
    } else {
        format!("{}.{}", ident(schema), ident(&table.name))
    };
    let column = |key: &str| {
        if table.columns.iter().any(|c| c.name == key) {
            Ok(ident(key))
        } else {
            Err(format!("`{key}` is not a column of {}, so it cannot be edited.", table.name))
        }
    };
    let keys = || {
        row_key(table).ok_or_else(|| {
            "This table has no primary key or NOT NULL unique key, so its rows cannot be \
             matched safely from the grid. Use the console instead."
                .to_owned()
        })
    };
    let matcher = |keys: &[String], row: &[serde_json::Value]| -> Result<String, String> {
        let terms = keys
            .iter()
            .map(|key| {
                let i = columns.iter().position(|c| c == key).ok_or_else(|| {
                    format!("The result does not include key column `{key}`; run `select *` first.")
                })?;
                match row.get(i) {
                    Some(serde_json::Value::Null) | None => {
                        Err("A key value is NULL, so the row cannot be matched.".to_owned())
                    }
                    cell => Ok(format!("{} = {}", ident(key), sql_literal(dialect, cell)?)),
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(format!("({})", terms.join(" and ")))
    };

    let mut statements = Vec::new();
    if !changes.deletes.is_empty() {
        let keys = keys()?;
        for chunk in changes.deletes.chunks(DELETE_CHUNK) {
            let matches =
                chunk.iter().map(|row| matcher(&keys, row)).collect::<Result<Vec<_>, _>>()?;
            statements.push((
                format!("delete from {name} where {}", matches.join(" or ")),
                Some(chunk.len() as u64),
            ));
        }
    }
    if !changes.updates.is_empty() {
        let keys = keys()?;
        for update in &changes.updates {
            if update.values.is_empty() {
                continue;
            }
            let sets = update
                .values
                .iter()
                .map(|(key, value)| {
                    Ok(format!("{} = {}", column(key)?, sql_literal(dialect, Some(value))?))
                })
                .collect::<Result<Vec<_>, String>>()?;
            statements.push((
                format!(
                    "update {name} set {} where {}",
                    sets.join(", "),
                    matcher(&keys, &update.row)?
                ),
                Some(1),
            ));
        }
    }
    for insert in &changes.inserts {
        if insert.is_empty() {
            let statement = if matches!(dialect, "mysql" | "mariadb") {
                format!("insert into {name} () values ()")
            } else {
                format!("insert into {name} default values")
            };
            statements.push((statement, Some(1)));
            continue;
        }
        let names = insert.keys().map(|key| column(key)).collect::<Result<Vec<_>, _>>()?;
        let values = insert
            .values()
            .map(|value| sql_literal(dialect, Some(value)))
            .collect::<Result<Vec<_>, _>>()?;
        statements.push((
            format!("insert into {name} ({}) values ({})", names.join(", "), values.join(", ")),
            Some(1),
        ));
    }
    if statements.is_empty() {
        return Err("There are no changes to save.".into());
    }
    Ok(statements)
}

/// A grid cell as a SQL literal; a missing cell is NULL. MySQL strings go hex-encoded with a
/// charset introducer, so neither quotes nor backslashes need escaping whatever
/// `NO_BACKSLASH_ESCAPES` says.
fn sql_literal(dialect: &str, cell: Option<&serde_json::Value>) -> Result<String, String> {
    use serde_json::Value as J;
    let tsql = matches!(dialect, "mssql" | "sybase");
    match cell {
        Some(J::Bool(b)) if tsql => Ok(if *b { "1" } else { "0" }.into()),
        Some(J::Bool(b)) => Ok(if *b { "TRUE" } else { "FALSE" }.into()),
        Some(J::Number(n)) => Ok(n.to_string()),
        Some(J::String(text)) if matches!(dialect, "mysql" | "mariadb") => {
            if text.is_empty() {
                return Ok("''".into());
            }
            let hex: String = text.bytes().map(|b| format!("{b:02X}")).collect();
            Ok(format!("_utf8mb4 X'{hex}'"))
        }
        Some(J::String(text)) if tsql => Ok(format!("N'{}'", text.replace('\'', "''"))),
        Some(J::String(text)) => Ok(format!("'{}'", text.replace('\'', "''"))),
        Some(J::Null) | None => Ok("NULL".into()),
        Some(_) => Err("Binary values cannot be written from the grid.".into()),
    }
}

/// How to open, commit and roll back a transaction, for the databases the grid saves in one.
fn transaction_verbs(dialect: &str) -> Option<[&'static str; 3]> {
    match dialect {
        "pg" | "cockroach" | "greenplum" | "redshift" | "sqlite" => {
            Some(["BEGIN", "COMMIT", "ROLLBACK"])
        }
        "mysql" | "mariadb" => Some(["START TRANSACTION", "COMMIT", "ROLLBACK"]),
        "mssql" | "sybase" => {
            Some(["BEGIN TRANSACTION", "COMMIT TRANSACTION", "ROLLBACK TRANSACTION"])
        }
        _ => None,
    }
}

/// Identifier quote characters per dialect.
fn quote_pair(dialect: &str) -> (&'static str, &'static str) {
    match dialect {
        "mysql" | "mariadb" | "clickhouse" | "hive" | "spark" | "bigquery" | "couchbase" => {
            ("`", "`")
        }
        "mssql" | "sybase" => ("[", "]"),
        _ => ("\"", "\""),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn keyed_table(
        constraint: rowcraft_model::ConstraintKind,
        nullable: bool,
    ) -> rowcraft_model::Table {
        let column = |name: &str| rowcraft_model::Column {
            name: name.into(),
            nullable: nullable || name != "id",
            ..Default::default()
        };
        rowcraft_model::Table {
            name: "t".into(),
            columns: vec![column("id"), column("name")],
            constraints: vec![rowcraft_model::Constraint {
                name: "k".into(),
                kind: constraint,
                columns: vec!["id".into()],
                definition: String::new(),
                reference: None,
            }],
            ..Default::default()
        }
    }

    fn changes(value: serde_json::Value) -> RowChanges {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn grid_changes_become_keyed_statements_with_expected_row_counts() {
        use rowcraft_model::ConstraintKind::{PrimaryKey, Unique};
        let table = keyed_table(PrimaryKey, false);
        let columns = vec!["id".to_owned(), "name".to_owned()];
        let all = changes(json!({
            "deletes": [[1, "a'b"], [2, "c"]],
            "updates": [{ "row": [3, "x"], "values": { "name": "it's", "id": 30 } }],
            "inserts": [{ "name": null }, {}],
        }));
        assert_eq!(
            change_statements("pg", "public", &table, &columns, &all).unwrap(),
            [
                (r#"delete from "public"."t" where ("id" = 1) or ("id" = 2)"#.to_owned(), Some(2)),
                (
                    r#"update "public"."t" set "name" = 'it''s', "id" = 30 where ("id" = 3)"#
                        .into(),
                    Some(1)
                ),
                (r#"insert into "public"."t" ("name") values (NULL)"#.into(), Some(1)),
                (r#"insert into "public"."t" default values"#.into(), Some(1)),
            ]
        );
        let mysql = changes(
            json!({ "updates": [{ "row": [1, "a"], "values": { "name": "a'b" } }], "inserts": [{}] }),
        );
        assert_eq!(
            change_statements("mysql", "shop", &table, &columns, &mysql).unwrap(),
            [
                (
                    "update `shop`.`t` set `name` = _utf8mb4 X'612762' where (`id` = 1)".to_owned(),
                    Some(1)
                ),
                ("insert into `shop`.`t` () values ()".into(), Some(1)),
            ]
        );
        let tsql = changes(json!({ "updates": [{ "row": [1, "a"], "values": { "name": "é" } }] }));
        assert_eq!(
            change_statements("mssql", "dbo", &table, &columns, &tsql).unwrap()[0].0,
            "update [dbo].[t] set [name] = N'é' where ([id] = 1)"
        );
        // A NOT NULL unique key stands in for a missing primary key; a nullable one does not.
        let deletes = changes(json!({ "deletes": [[1, "a"]] }));
        assert!(
            change_statements("sqlite", "", &keyed_table(Unique, false), &columns, &deletes)
                .is_ok()
        );
        assert!(
            change_statements("sqlite", "", &keyed_table(Unique, true), &columns, &deletes)
                .is_err()
        );
        // Inserts need no key at all.
        let insert = changes(json!({ "inserts": [{ "name": "n" }] }));
        assert!(
            change_statements("sqlite", "", &keyed_table(Unique, true), &columns, &insert).is_ok()
        );
    }

    #[test]
    fn grid_changes_are_refused_when_they_cannot_be_applied_exactly() {
        let table = keyed_table(rowcraft_model::ConstraintKind::PrimaryKey, false);
        let columns = vec!["id".to_owned(), "name".to_owned()];
        let refused = |dialect: &str, columns: &[String], value| {
            change_statements(dialect, "s", &table, columns, &changes(value)).is_err()
        };
        assert!(refused("pg", &columns, json!({})));
        assert!(refused("pg", &["name".to_owned()], json!({ "deletes": [["a"]] })));
        assert!(refused("pg", &columns, json!({ "deletes": [[null, "a"]] })));
        assert!(refused(
            "pg",
            &columns,
            json!({ "updates": [{ "row": [1, "a"], "values": { "nope": 1 } }] })
        ));
        assert!(refused("pg", &columns, json!({ "inserts": [{ "total": 1 }] })));
        assert!(refused("mongo", &columns, json!({ "deletes": [[1, "a"]] })));
    }

    #[test]
    fn mongo_collection_preview_uses_native_commands() {
        let f = Fixture::new("mongo-preview");
        f.bench.state().dialect = Some("mongo".into());
        assert_eq!(
            f.bench.preview_statement("communitiesDb", "shares").unwrap(),
            "db.getCollection(\"shares\").find({}).limit(2000)"
        );
        assert_eq!(schema_statement("mongo", "communitiesDb").unwrap(), "use communitiesDb");
        assert_eq!(
            f.bench.preview_statement("communitiesDb", "a\"b").unwrap(),
            "db.getCollection(\"a\\\"b\").find({}).limit(2000)"
        );
        assert!(schema_statement("mongo", "db;drop").is_err());
        f.bench.state().dialect = Some("redis".into());
        assert!(f.bench.preview_statement("0", "session").unwrap().starts_with("SCAN 0 MATCH"));
        assert_eq!(schema_statement("redis", "2").unwrap(), "SELECT 2");
        assert!(schema_statement("redis", "2\nFLUSHALL").is_err());
        f.bench.state().dialect = Some("couchbase".into());
        assert_eq!(
            f.bench.preview_statement("bucket.scope", "col`lection").unwrap(),
            "select * from `bucket`.`scope`.`col``lection`"
        );
        f.bench.state().dialect = Some("dynamo".into());
        assert_eq!(
            f.bench.preview_statement("us-east-1", "items").unwrap(),
            "select * from \"items\""
        );
        f.bench.state().dialect = Some("neo4j".into());
        assert_eq!(
            f.bench.preview_statement("graph", "User`Label").unwrap(),
            "MATCH (n:`User``Label`) RETURN n LIMIT 2000"
        );
    }

    #[tokio::test]
    async fn edits_sources_in_place_preserves_siblings_and_reopens_with_saved_changes() {
        let f = Fixture::new("edit-source");
        f.bench.add_source(&f.sqlite_draft("local")).unwrap();
        f.bench.add_source(&f.sqlite_draft("other")).unwrap();
        f.bench.connect("local").await.unwrap();
        let mut draft = f.bench.edit_source("local").unwrap().draft;
        draft.name = "renamed".into();
        draft.comment = "Updated connection".into();
        draft.color = "#123456".into();
        draft.read_only = true;
        draft.schema_pattern = "*:*".into();
        draft.properties = "application_name=reporting".into();
        f.bench.test_source_connection(&draft, Some("local")).await.unwrap();
        let updated = f.bench.update_source("local", &draft).unwrap();
        assert_eq!(updated.name, "renamed");
        assert_eq!(
            updated.sources.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["renamed", "other"]
        );
        assert!(f.bench.run("SELECT 1").await.unwrap_err().contains("Select a data source"));
        assert!(f.bench.edit_source("local").is_err());
        let reopened = Workbench::open(f.bench.engine.clone(), f.bench.paths.clone(), Vec::new());
        assert_eq!(reopened.edit_source("renamed").unwrap().draft, draft);
        reopened.connect("renamed").await.unwrap();
        assert_eq!(reopened.run("SELECT 1").await.unwrap().rows, vec![vec![json!(1)]]);
        let before = std::fs::read(&f.bench.paths.datasources).unwrap();
        draft.name = "other".into();
        assert!(f.bench.update_source("renamed", &draft).unwrap_err().contains("already exists"));
        assert_eq!(std::fs::read(&f.bench.paths.datasources).unwrap(), before);
        assert!(f.bench.update_source("missing", &draft).is_err());
    }

    #[test]
    fn editing_preserves_keychain_references_without_loading_or_exposing_passwords() {
        let f = Fixture::new("edit-credentials");
        let mut source = f.sqlite_draft("local").validate(&["sqlite"], &[]).unwrap();
        source.password_keychain = Some("fixture-database-account".into());
        source.ssl_files.key_password.account = Some("fixture-ssl-account".into());
        DataSourceFile { datasources: vec![source.clone()] }
            .save(&f.bench.paths.datasources)
            .unwrap();
        let defaults = f.bench.edit_source("local").unwrap();
        assert!(defaults.has_saved_password && defaults.has_saved_key_password);
        let serialized = serde_json::to_string(&defaults).unwrap();
        assert!(!serialized.contains("fixture-database-account"));
        assert!(!serialized.contains("fixture-ssl-account"));
        assert!(defaults.draft.password.is_empty() && defaults.draft.key_password.is_empty());
        let mut draft = defaults.draft;
        draft.comment = "Only the comment changed".into();
        f.bench.update_source("local", &draft).unwrap();
        let saved = DataSourceFile::load(&f.bench.paths.datasources).unwrap();
        assert_eq!(saved.datasources[0].password_keychain, source.password_keychain);
        assert_eq!(saved.datasources[0].ssl_files.key_password, source.ssl_files.key_password);
    }

    #[test]
    fn schema_selection_quotes_identifiers_and_rejects_unsupported_drivers() {
        assert_eq!(schema_statement("mysql", "app").unwrap(), "USE `app`");
        assert_eq!(
            schema_statement("mysql", "a`; drop table t; --").unwrap(),
            "USE `a``; drop table t; --`"
        );
        assert_eq!(
            schema_statement("pg", "odd\"name").unwrap(),
            "SET search_path TO \"odd\"\"name\""
        );
        assert!(schema_statement("sqlite", "main").is_err());
    }

    #[test]
    fn ssh_configs_can_be_saved_without_a_database_and_survive_reopening() {
        let f = Fixture::new("save-ssh-independent");
        let draft = Draft {
            ssh_new_name: "Staging".into(),
            ssh_host: "ssh.example.test".into(),
            ssh_user: "demo".into(),
            ssh_port: "2525".into(),
            ssh_auth: rowcraft_config::SshAuth::Agent,
            ..Default::default()
        };
        assert_eq!(f.bench.save_ssh_config(&draft).unwrap(), ["Staging"]);
        assert!(f.bench.bootstrap().sources.is_empty());
        assert!(!f.bench.paths.datasources.exists());
        let saved = SshFile::load(&f.bench.paths.ssh).unwrap();
        assert_eq!(saved.configs[0].port, Some(2525));
        let reopened = Workbench::open(f.bench.engine.clone(), f.bench.paths.clone(), Vec::new());
        assert_eq!(reopened.form_defaults().ssh_names, ["Staging"]);
        assert!(reopened.save_ssh_config(&draft).unwrap_err().contains("already exists"));
        assert_eq!(SshFile::load(&f.bench.paths.ssh).unwrap(), saved);
    }

    struct Fixture {
        dir: PathBuf,
        bench: Workbench,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("rowcraft-wb-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let paths =
                Paths { datasources: dir.join("datasources.toml"), ssh: dir.join("ssh.toml") };
            let engine = Arc::new(
                Engine::new(rowcraft_registry::default_registry()).with_ssh_file(paths.ssh.clone()),
            );
            Self { bench: Workbench::open(engine, paths, Vec::new()), dir }
        }

        fn sqlite_draft(&self, name: &str) -> Draft {
            let mut draft = Draft::initial();
            draft.select_driver("sqlite");
            draft.name = name.into();
            draft.database = self.dir.join("test.sqlite").display().to_string();
            draft
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn a_broken_file_and_driver_warnings_reach_the_status_bar() {
        let f = Fixture::new("broken");
        std::fs::write(&f.bench.paths.datasources, "this is = not [valid").unwrap();
        let engine = f.bench.engine.clone();
        let paths = f.bench.paths.clone();

        let broken = Workbench::open(engine.clone(), paths.clone(), vec!["bad driver".into()]);
        let boot = broken.bootstrap();
        assert!(boot.sources.is_empty());
        let error = boot.startup_error.unwrap();
        assert!(error.contains("drivers.toml: bad driver"), "{error}");
        assert!(error.contains('\n'), "both problems are reported: {error}");

        std::fs::remove_file(&paths.datasources).unwrap();
        let clean = Workbench::open(engine, paths, Vec::new());
        assert_eq!(clean.bootstrap().startup_error, None);
    }

    #[test]
    fn adding_a_source_saves_it_and_lists_it() {
        let f = Fixture::new("add");
        assert!(f.bench.bootstrap().sources.is_empty());

        let added = f.bench.add_source(&f.sqlite_draft("local")).unwrap();
        assert_eq!(added.name, "local");
        assert_eq!(added.sources.len(), 1);
        assert_eq!(added.sources[0].dialect, "sqlite");
        assert_eq!(f.bench.bootstrap().sources, added.sources);
        assert!(std::fs::read_to_string(&f.bench.paths.datasources).unwrap().contains("local"));

        let again = f.bench.add_source(&f.sqlite_draft("local")).unwrap_err();
        assert!(again.contains("already exists"), "{again}");
    }

    #[test]
    fn a_new_ssh_configuration_is_saved_with_the_source_that_names_it() {
        let f = Fixture::new("ssh");
        let mut draft = f.sqlite_draft("tunneled");
        draft.use_ssh = true;
        draft.ssh_new = true;
        draft.ssh_new_name = "bastion".into();
        draft.ssh_host = "ssh.example.test".into();
        draft.ssh_user = "me".into();
        draft.ssh_auth = rowcraft_config::SshAuth::Agent;
        f.bench.add_source(&draft).unwrap();

        assert_eq!(f.bench.form_defaults().ssh_names, ["bastion"]);
        assert_eq!(f.bench.bootstrap().sources[0].ssh_tunnel.as_deref(), Some("bastion"));
    }

    #[test]
    fn an_invalid_form_saves_nothing() {
        let f = Fixture::new("invalid");
        let err = f.bench.add_source(&Draft::initial()).unwrap_err();
        assert_eq!(err, "Name is required.");
        assert!(!f.bench.paths.datasources.exists());
    }

    #[test]
    fn the_form_starts_on_postgresql() {
        let f = Fixture::new("defaults");
        let defaults = f.bench.form_defaults();
        assert_eq!(defaults.draft.driver, "postgresql");
        assert_eq!(defaults.draft.host, "localhost");
        assert!(defaults.ssh_names.is_empty());
    }

    #[test]
    fn custom_driver_lifecycle_persists_edits_and_protects_sources() {
        let f = Fixture::new("driver-lifecycle");
        let mut driver = CustomDriverConfig {
            id: "wb-custom-sqlite".into(),
            name: "Custom SQLite".into(),
            dialect: "sqlite".into(),
            port: None,
            target: rowcraft_config::DriverTarget::File,
            jdbc_class: None,
            jdbc_jars: vec![],
            jdbc_url_template: None,
        };
        f.bench.save_driver(driver.clone()).unwrap();
        driver.name = "Renamed SQLite".into();
        f.bench.save_driver(driver.clone()).unwrap();
        assert_eq!(f.bench.driver_settings().unwrap().custom, vec![driver.clone()]);
        let mut draft = f.sqlite_draft("custom-source");
        draft.select_driver(&driver.id);
        f.bench.add_source(&draft).unwrap();
        assert!(f.bench.remove_driver(&driver.id).unwrap_err().contains("used by"));
        f.bench.remove_source("custom-source").unwrap();
        f.bench.remove_driver(&driver.id).unwrap();
        assert!(f.bench.driver_settings().unwrap().custom.is_empty());
        driver.id = "sqlite".into();
        assert!(f.bench.save_driver(driver).unwrap_err().contains("Built-in"));
    }

    #[tokio::test]
    async fn unrestricted_queries_default_to_2000_and_show_all_removes_the_limit() {
        let f = Fixture::new("default-limit");
        f.bench.add_source(&f.sqlite_draft("local")).unwrap();
        f.bench.connect("local").await.unwrap();
        let sql = "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 2505) SELECT x FROM n";
        let preview = f.bench.run(sql).await.unwrap();
        assert_eq!(preview.rows.len(), 2000);
        assert_eq!(preview.row_limit, Some(2000));
        assert!(preview.summary.contains("Default limit: 2000 rows"));
        let all = f.bench.run_with_limit(sql, None, true).await.unwrap();
        assert_eq!(all.rows.len(), 2505);
        assert_eq!(all.row_limit, None);
        let explicit = f.bench.run(&format!("{sql} LIMIT 5")).await.unwrap();
        assert_eq!(explicit.rows.len(), 5);
        assert_eq!(explicit.row_limit, None);
    }

    #[tokio::test]
    async fn switching_sources_keeps_both_sessions_and_their_metadata() {
        let f = Fixture::new("persistent-sessions");
        f.bench.add_source(&f.sqlite_draft("alpha")).unwrap();
        f.bench.add_source(&f.sqlite_draft("beta")).unwrap();
        f.bench.connect("alpha").await.unwrap();
        f.bench.run("CREATE TEMP TABLE session_marker(value TEXT)").await.unwrap();
        f.bench.run("INSERT INTO session_marker VALUES ('alpha')").await.unwrap();
        let alpha = f.bench.state().session.clone().unwrap();
        let catalog = f.bench.introspect().await.unwrap();
        f.bench.connect("beta").await.unwrap();
        f.bench.run("CREATE TEMP TABLE session_marker(value TEXT)").await.unwrap();
        f.bench.run("INSERT INTO session_marker VALUES ('beta')").await.unwrap();
        f.bench.connect("alpha").await.unwrap();
        assert!(Arc::ptr_eq(&alpha, f.bench.state().session.as_ref().unwrap()));
        assert_eq!(f.bench.state().catalog.as_ref(), Some(&catalog));
        assert_eq!(
            f.bench.run("SELECT value FROM session_marker").await.unwrap().rows,
            vec![vec![json!("alpha")]]
        );
        f.bench.connect("beta").await.unwrap();
        assert_eq!(
            f.bench.run("SELECT value FROM session_marker").await.unwrap().rows,
            vec![vec![json!("beta")]]
        );
    }

    #[tokio::test]
    async fn explicit_reconnect_and_source_edits_invalidate_only_the_affected_session() {
        let f = Fixture::new("session-invalidation");
        f.bench.add_source(&f.sqlite_draft("alpha")).unwrap();
        f.bench.add_source(&f.sqlite_draft("beta")).unwrap();
        f.bench.connect("alpha").await.unwrap();
        f.bench.run("CREATE TEMP TABLE alpha_temp(value TEXT)").await.unwrap();
        let alpha = f.bench.state().session.clone().unwrap();
        f.bench.connect("beta").await.unwrap();
        let beta = f.bench.state().session.clone().unwrap();
        f.bench.reconnect("alpha").await.unwrap();
        assert!(!Arc::ptr_eq(&alpha, f.bench.state().session.as_ref().unwrap()));
        assert!(f.bench.run("SELECT * FROM alpha_temp").await.is_err());
        f.bench.connect("beta").await.unwrap();
        assert!(Arc::ptr_eq(&beta, f.bench.state().session.as_ref().unwrap()));
        let draft = f.bench.edit_source("alpha").unwrap().draft;
        f.bench.update_source("alpha", &draft).unwrap();
        assert!(!f.bench.state().sessions.contains_key("alpha"));
        assert!(f.bench.state().sessions.contains_key("beta"));
        f.bench.remove_source("alpha").unwrap();
        assert!(Arc::ptr_eq(&beta, f.bench.state().session.as_ref().unwrap()));
    }

    #[tokio::test]
    async fn failed_connection_keeps_other_sources_available_and_duplicate_opens_are_coalesced() {
        let f = Fixture::new("session-failure");
        f.bench.add_source(&f.sqlite_draft("alpha")).unwrap();
        let mut bad = f.sqlite_draft("bad");
        bad.database = f.dir.join("missing-parent").join("db.sqlite").display().to_string();
        f.bench.add_source(&bad).unwrap();
        let (first, second) = tokio::join!(f.bench.connect("alpha"), f.bench.connect("alpha"));
        first.unwrap();
        second.unwrap();
        assert_eq!(f.bench.state().sessions.len(), 1);
        f.bench.run("CREATE TEMP TABLE marker(value TEXT)").await.unwrap();
        let alpha = f.bench.state().session.clone().unwrap();
        assert!(f.bench.connect("bad").await.is_err());
        f.bench.connect("alpha").await.unwrap();
        assert!(Arc::ptr_eq(&alpha, f.bench.state().session.as_ref().unwrap()));
        assert!(f.bench.run("SELECT * FROM marker").await.is_ok());
    }

    #[tokio::test]
    async fn connect_and_run_a_statement() {
        let f = Fixture::new("run");
        assert_eq!(f.bench.run("select 1").await.unwrap_err(), "Select a data source first.");
        assert!(f.bench.connect("nope").await.unwrap_err().contains("No data source named"));

        f.bench.add_source(&f.sqlite_draft("local")).unwrap();
        assert_eq!(f.bench.test_connection(&f.sqlite_draft("")).await, Ok(()));
        assert_eq!(f.bench.connect("local").await.unwrap(), "local");

        let out = f.bench.run("select 1 as one, null as empty, 'x' as label").await.unwrap();
        assert_eq!(out.columns, ["one", "empty", "label"]);
        assert_eq!(out.rows, vec![vec![json!(1), serde_json::Value::Null, json!("x")]]);
        assert!(out.summary.starts_with("1 row in "), "{}", out.summary);

        let err = f.bench.run("selec 1").await.unwrap_err();
        assert!(!err.is_empty());
    }

    #[tokio::test]
    async fn grid_changes_are_saved_in_one_transaction_or_not_at_all() {
        let f = Fixture::new("row-changes");
        f.bench.add_source(&f.sqlite_draft("local")).unwrap();
        f.bench.connect("local").await.unwrap();
        f.bench
            .run("create table people(id integer primary key, name text not null, note text)")
            .await
            .unwrap();
        f.bench.run("insert into people values (1, 'Ada', null), (2, 'Bob', 'x')").await.unwrap();
        let schema = f.bench.introspect().await.unwrap().schemas[0].name.clone();
        let columns = ["id".to_owned(), "name".to_owned(), "note".to_owned()];
        let rows = || async {
            f.bench.run("select id, name, note from people order by id").await.unwrap().rows
        };

        let saved = changes(json!({
            "deletes": [[2, "Bob", "x"]],
            "updates": [{ "row": [1, "Ada", null], "values": { "name": "Ada L.", "note": "it's" } }],
            "inserts": [{ "id": "3", "name": "Cy" }],
        }));
        let out = f.bench.apply_row_changes(&schema, "people", &columns, &saved).await.unwrap();
        assert!(out.summary.starts_with("3 rows affected"), "{}", out.summary);
        assert_eq!(
            rows().await,
            [[json!(1), json!("Ada L."), json!("it's")], [json!(3), json!("Cy"), json!(null)]]
        );

        // Row 2 is gone, so its update matches nothing and the valid insert is rolled back too.
        let stale = changes(json!({
            "updates": [{ "row": [2, "Bob", "x"], "values": { "name": "B" } }],
            "inserts": [{ "id": 4, "name": "Di" }],
        }));
        let err = f.bench.apply_row_changes(&schema, "people", &columns, &stale).await.unwrap_err();
        assert!(err.contains("matched 0 row(s)"), "{err}");
        // A constraint failure rolls back as well.
        let invalid =
            changes(json!({ "inserts": [{ "id": 5, "name": "Ed" }, { "id": 6, "name": null }] }));
        assert!(f.bench.apply_row_changes(&schema, "people", &columns, &invalid).await.is_err());
        assert_eq!(rows().await.len(), 2);
    }

    #[tokio::test]
    async fn explorer_preview_and_removal() {
        let f = Fixture::new("explore");
        assert_eq!(f.bench.introspect().await.unwrap_err(), "Select a data source first.");
        assert_eq!(
            f.bench.preview_statement("main", "t").unwrap_err(),
            "Select a data source first."
        );

        f.bench.add_source(&f.sqlite_draft("local")).unwrap();
        f.bench.connect("local").await.unwrap();
        f.bench.run("create table people (id integer primary key, name text)").await.unwrap();

        let catalog = f.bench.introspect().await.unwrap();
        let table = catalog.schemas.iter().flat_map(|s| &s.tables).find(|t| t.name == "people");
        assert_eq!(table.map(|t| t.columns.len()), Some(2));

        let sql = f.bench.preview_statement("main", "people").unwrap();
        assert_eq!(sql, "select * from \"main\".\"people\"");
        assert_eq!(f.bench.run(&sql).await.unwrap().columns, ["id", "name"]);

        assert!(f.bench.remove_source("nope").unwrap_err().contains("nope"));
        assert!(f.bench.remove_source("local").unwrap().is_empty());
        assert!(f.bench.bootstrap().sources.is_empty());
        assert_eq!(f.bench.run("select 1").await.unwrap_err(), "Select a data source first.");
    }

    #[tokio::test]
    async fn ai_tools_query_separately_limit_rows_and_reject_writes() {
        let f = Fixture::new("ai-readonly");
        f.bench.add_source(&f.sqlite_draft("local")).unwrap();
        f.bench.connect("local").await.unwrap();
        f.bench.run("CREATE TABLE people (id INTEGER)").await.unwrap();
        f.bench.run("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 40) INSERT INTO people SELECT x FROM n").await.unwrap();
        let output = f
            .bench
            .ai_query("local", Some("main"), "SELECT id FROM people ORDER BY id")
            .await
            .unwrap();
        assert_eq!(output.rows.len(), 25);
        assert!(f.bench.ai_query("local", Some("main"), "DELETE FROM people").await.is_err());
        let count = f.bench.run("SELECT count(*) FROM people").await.unwrap();
        assert_eq!(count.rows, vec![vec![json!(40)]]);
        let catalog = f.bench.ai_schema("local", Some("main")).await.unwrap();
        assert!(catalog.schemas.iter().flat_map(|s| &s.tables).any(|t| t.name == "people"));
        assert_eq!(f.bench.state().session.as_ref().unwrap().name(), "local");
    }

    #[test]
    fn identifiers_are_quoted_per_dialect() {
        assert_eq!(quote_pair("mysql"), ("`", "`"));
        assert_eq!(quote_pair("mssql"), ("[", "]"));
        assert_eq!(quote_pair("pg"), ("\"", "\""));
    }
}
