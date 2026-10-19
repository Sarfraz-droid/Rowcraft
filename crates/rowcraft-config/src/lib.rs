//! Data source configuration and settings.
//!
//! Data sources live in `datasources.toml` under the config directory (see [`config_dir`]).
//! Secrets are never written to that file: data sources reference the OS credential store or
//! an environment variable (`password_env`), which the engine resolves at connect time.

mod drivers;
mod options;
pub mod passwords;
mod scope;
mod ssh;
mod url;

pub use drivers::{
    CustomDriverConfig, DriverFile, DriverTarget, drivers_path, register_custom_drivers,
};
pub use options::{
    AuthType, ConnectionOptions, IntrospectionLevel, IntrospectionOptions, Isolation, LoadSources,
    Properties, SessionControl, SslFiles, SwitchSchema, TransactionMode,
};
pub use scope::{Defaults, ObjectFilter, SchemaPattern};
pub use ssh::{HostKeyPolicy, SshAuth, SshConfig, SshFile, ssh_path};
pub use url::{UrlParts, parse as parse_url};

use std::path::{Path, PathBuf};

use options::is_default;

use serde::{Deserialize, Serialize};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read or write {}: {source}", path.display())]
    Io { path: PathBuf, source: std::io::Error },
    #[error("invalid config {}: {message}", path.display())]
    Parse { path: PathBuf, message: String },
    #[error("data source `{0}` not found")]
    NotFound(String),
    #[error("data source `{0}` already exists")]
    Duplicate(String),
    #[error("SSH configuration `{0}` not found")]
    SshNotFound(String),
    #[error("SSH configuration `{0}` already exists")]
    SshDuplicate(String),
    #[error("custom driver `{0}` not found")]
    DriverNotFound(String),
    #[error("driver id `{0}` is already taken")]
    DriverDuplicate(String),
    #[error("environment variable `{0}` (password_env) is not set")]
    PasswordEnvMissing(String),
    #[error("{0}")]
    PasswordStore(String),
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("invalid {field}: {message}")]
    InvalidSetting { field: &'static str, message: String },
}

/// TLS behaviour for a connection. Most dialects resolve `Auto` to `Disable` for loopback
/// hosts and `VerifyFull` otherwise. MongoDB follows its URI defaults: TLS is off for
/// ordinary hosts and `mongodb://`, and on for `mongodb+srv://` or explicit `tls=true`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SslMode {
    #[default]
    Auto,
    Disable,
    /// TLS without checking the server certificate.
    Require,
    /// TLS, checking the certificate chain but not the host name.
    VerifyCa,
    /// "Full Verification": chain and host name.
    VerifyFull,
}

/// How to reach a database.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataSourceConfig {
    pub name: String,
    /// Dialect id, e.g. `"pg"`. Must match `Dialect::id()`.
    pub dialect: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub user: Option<String>,
    /// Name of the environment variable that holds the password.
    pub password_env: Option<String>,
    /// Account in the OS credential store. The password itself is never serialized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_keychain: Option<String>,
    #[serde(default)]
    pub ssl_mode: SslMode,
    #[serde(default)]
    pub read_only: bool,

    // General tab
    /// Catalog driver id (`rowcraft-drivers`); the dialect alone is enough to connect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Sidebar colour, `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Free-form "Connection type" label such as `production`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_type: Option<String>,
    /// Overrides host, port, database and user when set; see [`DataSourceConfig::effective`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub auth: AuthType,

    // Options tab
    #[serde(default, skip_serializing_if = "is_default")]
    pub options: ConnectionOptions,
    #[serde(default, skip_serializing_if = "is_default")]
    pub introspection: IntrospectionOptions,

    // SSH/SSL tab
    #[serde(default, skip_serializing_if = "is_default")]
    pub ssl_files: SslFiles,
    /// Name of a shared SSH configuration (`ssh.toml`); the connection goes through its tunnel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_tunnel: Option<String>,
    /// "Forced local port": listen on this local port instead of a free one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_local_port: Option<u16>,

    // Schemas tab
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_filter: Option<String>,

    // Advanced tab
    #[serde(default, skip_serializing_if = "Properties::is_empty")]
    pub properties: Properties,

    /// Resolved at connect time, never serialized.
    #[serde(skip)]
    pub password: Option<String>,
}

impl std::fmt::Debug for DataSourceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataSourceConfig")
            .field("name", &self.name)
            .field("dialect", &self.dialect)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("user", &self.user)
            .field("password_env", &self.password_env)
            .field("ssl_mode", &self.ssl_mode)
            .field("read_only", &self.read_only)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .finish_non_exhaustive()
    }
}

impl DataSourceConfig {
    /// Resolves a saved credential or environment variable when no direct password is present.
    pub fn resolve_password(&mut self) -> Result<()> {
        if self.auth != AuthType::UserPassword || self.password.is_some() {
            return Ok(());
        }
        if let Some(account) = &self.password_keychain {
            self.password = Some(passwords::load(account).map_err(Error::PasswordStore)?);
            return Ok(());
        }
        if let Some(var) = &self.password_env {
            let value = std::env::var(var).map_err(|_| Error::PasswordEnvMissing(var.clone()))?;
            self.password = Some(value);
        }
        Ok(())
    }

    /// The settings to connect with: when `url` is set it overrides host, port, database and user,
    /// Fields the URL leaves out keep their own values.
    pub fn effective(&self) -> Result<Self> {
        let mut cfg = self.clone();
        if let Some(url) = self.url.as_deref().filter(|u| !u.trim().is_empty()) {
            let parts = parse_url(url).map_err(Error::InvalidUrl)?;
            cfg.host = parts.host.or(cfg.host);
            cfg.port = parts.port.or(cfg.port);
            cfg.database = parts.database.or(cfg.database);
            cfg.user = parts.user.or(cfg.user);
        }
        Ok(cfg)
    }

    /// Parsed Schemas tab pattern, `None` when the field is empty (everything).
    pub fn parsed_schema_pattern(&self) -> Result<Option<SchemaPattern>> {
        match self.schema_pattern.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            None => Ok(None),
            Some(p) => SchemaPattern::parse(p)
                .map(Some)
                .map_err(|message| Error::InvalidSetting { field: "schema pattern", message }),
        }
    }

    /// Parsed Schemas tab object filter, `None` when the field is empty.
    pub fn parsed_object_filter(&self) -> Result<Option<ObjectFilter>> {
        match self.object_filter.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            None => Ok(None),
            Some(p) => ObjectFilter::parse(p)
                .map(Some)
                .map_err(|message| Error::InvalidSetting { field: "object filter", message }),
        }
    }

    pub fn is_loopback_host(&self) -> bool {
        matches!(self.host.as_deref(), None | Some("localhost" | "127.0.0.1" | "::1"))
    }
}

/// Contents of `datasources.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataSourceFile {
    #[serde(default, rename = "datasource")]
    pub datasources: Vec<DataSourceConfig>,
}

impl DataSourceFile {
    pub fn get(&self, name: &str) -> Result<&DataSourceConfig> {
        self.datasources
            .iter()
            .find(|d| d.name == name)
            .ok_or_else(|| Error::NotFound(name.to_owned()))
    }

    pub fn add(&mut self, ds: DataSourceConfig) -> Result<()> {
        if self.datasources.iter().any(|d| d.name == ds.name) {
            return Err(Error::Duplicate(ds.name));
        }
        self.datasources.push(ds);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<DataSourceConfig> {
        let idx = self
            .datasources
            .iter()
            .position(|d| d.name == name)
            .ok_or_else(|| Error::NotFound(name.to_owned()))?;
        Ok(self.datasources.remove(idx))
    }

    /// A missing file is an empty config.
    pub fn load(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => return Err(Error::Io { path: path.to_owned(), source }),
        };
        toml::from_str(&text)
            .map_err(|e| Error::Parse { path: path.to_owned(), message: e.to_string() })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let io = |source| Error::Io { path: path.to_owned(), source };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| Error::Parse { path: path.to_owned(), message: e.to_string() })?;
        std::fs::write(path, text).map_err(io)
    }
}

/// `$ROWCRAFT_CONFIG_DIR`, else `$XDG_CONFIG_HOME/rowcraft`, else `~/.config/rowcraft`
/// (`%APPDATA%\rowcraft` on Windows).
pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ROWCRAFT_CONFIG_DIR") {
        return dir.into();
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("rowcraft");
    }
    if cfg!(windows) {
        let base = std::env::var_os("APPDATA").unwrap_or_default();
        return PathBuf::from(base).join("rowcraft");
    }
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config").join("rowcraft")
}

pub fn datasources_path() -> PathBuf {
    config_dir().join("datasources.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str) -> DataSourceConfig {
        DataSourceConfig {
            name: name.into(),
            dialect: "pg".into(),
            host: Some("localhost".into()),
            port: Some(5432),
            database: Some("app".into()),
            user: Some("me".into()),
            password_env: Some("PW".into()),
            ..Default::default()
        }
    }

    #[test]
    fn round_trips_through_toml() {
        let mut file = DataSourceFile::default();
        file.add(sample("a")).unwrap();
        file.add(sample("b")).unwrap();
        let text = toml::to_string_pretty(&file).unwrap();
        assert!(text.contains("[[datasource]]"));
        assert_eq!(toml::from_str::<DataSourceFile>(&text).unwrap(), file);
    }

    #[test]
    fn rejects_duplicates_and_reports_missing() {
        let mut file = DataSourceFile::default();
        file.add(sample("a")).unwrap();
        assert!(matches!(file.add(sample("a")), Err(Error::Duplicate(_))));
        assert!(matches!(file.get("zzz"), Err(Error::NotFound(_))));
        assert!(file.remove("a").is_ok());
    }

    #[test]
    fn password_is_never_serialized_or_debug_printed() {
        let mut ds = sample("a");
        ds.password = Some("hunter2".into());
        assert!(!toml::to_string(&ds).unwrap().contains("hunter2"));
        assert!(!format!("{ds:?}").contains("hunter2"));
    }

    #[test]
    fn missing_password_env_is_an_error() {
        let mut ds = sample("a");
        ds.password_env = Some("ROWCRAFT_TEST_SURELY_UNSET_VAR".into());
        assert!(matches!(ds.resolve_password(), Err(Error::PasswordEnvMissing(_))));
    }

    #[test]
    fn load_of_missing_file_is_empty() {
        let path = std::env::temp_dir().join("rowcraft-no-such-dir/none.toml");
        assert!(DataSourceFile::load(&path).unwrap().datasources.is_empty());
    }

    #[test]
    fn old_files_without_the_new_options_still_load_with_defaults() {
        let text = r#"
            [[datasource]]
            name = "old"
            dialect = "pg"
            host = "localhost"
        "#;
        let file: DataSourceFile = toml::from_str(text).unwrap();
        let ds = &file.datasources[0];
        assert_eq!(ds.options, ConnectionOptions::default());
        assert_eq!(ds.introspection, IntrospectionOptions::default());
        assert!(ds.introspection.auto_sync && ds.properties.is_empty() && ds.url.is_none());
    }

    #[test]
    fn defaults_are_not_written_to_the_file() {
        let text =
            toml::to_string_pretty(&DataSourceFile { datasources: vec![sample("a")] }).unwrap();
        for noise in ["options", "introspection", "ssl_files", "properties", "url", "comment"] {
            assert!(!text.contains(noise), "{noise} leaked into:\n{text}");
        }
    }

    #[test]
    fn every_option_round_trips() {
        let mut ds = sample("full");
        ds.driver = Some("postgresql".into());
        ds.comment = Some("prod replica".into());
        ds.color = Some("#cc0000".into());
        ds.connection_type = Some("production".into());
        ds.url = Some("postgresql://h:1/d".into());
        ds.auth = AuthType::NoAuth;
        ds.ssl_mode = SslMode::VerifyCa;
        ds.options = ConnectionOptions {
            transaction: TransactionMode::Manual,
            isolation: Isolation::Serializable,
            switch_schema: SwitchSchema::Disabled,
            time_zone: Some("UTC".into()),
            keep_alive_secs: Some(60),
            keep_alive_query: Some("select 1".into()),
            auto_disconnect_secs: Some(300),
            single_database_mode: true,
            session_control: SessionControl::SeparateIntrospection,
            startup_script: Some("set search_path = a;\nset x = 1;".into()),
        };
        ds.introspection = IntrospectionOptions {
            auto_sync: false,
            load_sources: LoadSources::None,
            warn_outdated: false,
            level: IntrospectionLevel::Names,
            external_data_file: Some("virtual.xml".into()),
        };
        ds.ssl_files = SslFiles {
            ca_file: Some("ca.pem".into()),
            cert_file: Some("c.pem".into()),
            key_file: Some("k.pem".into()),
            key_password_env: Some("KEY_PW".into()),
            ..Default::default()
        };
        ds.ssh_tunnel = Some("jump".into());
        ds.ssh_local_port = Some(15432);
        ds.schema_pattern = Some("*:public".into());
        ds.object_filter = Some("table:-tmp.*".into());
        ds.properties.insert("application_name".into(), "rowcraft".into());
        let file = DataSourceFile { datasources: vec![ds] };
        let text = toml::to_string_pretty(&file).unwrap();
        assert_eq!(toml::from_str::<DataSourceFile>(&text).unwrap(), file, "{text}");
    }

    #[test]
    fn url_overrides_the_fields_it_names_and_keeps_the_rest() {
        let mut ds = sample("a");
        ds.url = Some("jdbc:postgresql://other:6000/db2".into());
        let eff = ds.effective().unwrap();
        assert_eq!(
            (eff.host.as_deref(), eff.port, eff.database.as_deref(), eff.user.as_deref()),
            (Some("other"), Some(6000), Some("db2"), Some("me")),
        );
        ds.url = Some("postgresql://h".into());
        let eff = ds.effective().unwrap();
        assert_eq!(
            (eff.host.as_deref(), eff.port, eff.database.as_deref()),
            (Some("h"), Some(5432), Some("app"))
        );
        ds.url = Some("nonsense".into());
        assert!(matches!(ds.effective(), Err(Error::InvalidUrl(_))));
        ds.url = Some("   ".into());
        assert_eq!(ds.effective().unwrap().host.as_deref(), Some("localhost"));
    }

    #[test]
    fn schema_pattern_and_filter_are_parsed_on_demand() {
        let mut ds = sample("a");
        assert!(
            ds.parsed_schema_pattern().unwrap().is_none()
                && ds.parsed_object_filter().unwrap().is_none()
        );
        ds.schema_pattern = Some("@:@|*:{tmp.*}".into());
        assert!(ds.parsed_schema_pattern().unwrap().is_some());
        ds.schema_pattern = Some("*:{(}".into());
        assert!(matches!(
            ds.parsed_schema_pattern(),
            Err(Error::InvalidSetting { field: "schema pattern", .. })
        ));
        ds.object_filter = Some("table:(".into());
        assert!(matches!(
            ds.parsed_object_filter(),
            Err(Error::InvalidSetting { field: "object filter", .. })
        ));
    }
}
