//! The "Add data source" form's model: what the user typed, its validation, and saving.
//! The frontend owns the widgets and sends the whole [`Draft`] as JSON; the rules live here so
//! they are unit-tested. Tabs and labels are documented (docs/data-source-options.md).

use std::path::Path;

use rowcraft_config::{
    AuthType, ConnectionOptions, DataSourceConfig, DataSourceFile, HostKeyPolicy,
    IntrospectionLevel, IntrospectionOptions, Isolation, LoadSources, Properties, SessionControl,
    SshAuth, SshConfig, SshFile, SslFiles, SslMode, SwitchSchema, TransactionMode, parse_url,
};
use rowcraft_drivers::Target;
use serde::{Deserialize, Serialize};

/// Raw form content. Free text is kept as typed; choices and checkboxes are typed values.
/// A field missing from the JSON takes its default, so the frontend may send a partial draft.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Draft {
    // General
    pub name: String,
    pub comment: String,
    pub color: String,
    pub connection_type: String,
    /// Catalog driver id; empty means "use `dialect` as typed".
    pub driver: String,
    pub dialect: String,
    pub host: String,
    pub port: String,
    pub database: String,
    pub user: String,
    /// Name of the environment variable holding the password, never the password itself.
    pub password_env: String,
    /// Entered password; passed to the engine and credential store without trimming.
    pub password: String,
    pub clear_password: bool,
    pub url: String,
    pub auth: AuthType,
    // Options: connection
    pub read_only: bool,
    pub transaction: TransactionMode,
    pub isolation: Isolation,
    pub switch_schema: SwitchSchema,
    pub session_control: SessionControl,
    pub single_database_mode: bool,
    pub time_zone: String,
    pub keep_alive_secs: String,
    pub keep_alive_query: String,
    pub auto_disconnect_secs: String,
    pub startup_script: String,
    // Options: introspection
    pub auto_sync: bool,
    pub load_sources: LoadSources,
    pub warn_outdated: bool,
    pub level: IntrospectionLevel,
    pub external_data_file: String,
    // SSH/SSL
    pub ssl_mode: SslMode,
    pub ca_file: String,
    pub cert_file: String,
    pub key_file: String,
    pub key_password_env: String,
    pub key_password: String,
    pub clear_key_password: bool,
    // Schemas
    pub schema_pattern: String,
    pub object_filter: String,
    // Advanced: one `name=value` per line
    pub properties: String,
    // SSH tunnel: either an existing shared configuration or a new one typed in below
    pub use_ssh: bool,
    pub ssh_existing: String,
    pub ssh_new: bool,
    pub ssh_local_port: String,
    pub ssh_new_name: String,
    pub ssh_host: String,
    pub ssh_port: String,
    pub ssh_user: String,
    pub ssh_auth: SshAuth,
    pub ssh_key_file: String,
    pub ssh_password_env: String,
    pub ssh_password: String,
    pub ssh_passphrase_env: String,
    pub ssh_passphrase: String,
    pub ssh_host_key: HostKeyPolicy,
    pub ssh_known_hosts_file: String,
}

/// What a valid form produces: the data source and, when the user typed a new SSH
/// configuration, that too (to be saved before the data source that names it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    pub config: DataSourceConfig,
    pub new_ssh: Option<SshConfig>,
}

impl std::fmt::Debug for Draft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Draft")
            .field("name", &self.name)
            .field("driver", &self.driver)
            .field("dialect", &self.dialect)
            .field("password", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl Default for Draft {
    fn default() -> Self {
        let options = ConnectionOptions::default();
        let introspection = IntrospectionOptions::default();
        Self {
            name: String::new(),
            comment: String::new(),
            color: String::new(),
            connection_type: String::new(),
            driver: String::new(),
            dialect: String::new(),
            host: String::new(),
            port: String::new(),
            database: String::new(),
            user: String::new(),
            password_env: String::new(),
            password: String::new(),
            clear_password: false,
            url: String::new(),
            auth: AuthType::default(),
            read_only: false,
            transaction: options.transaction,
            isolation: options.isolation,
            switch_schema: options.switch_schema,
            session_control: options.session_control,
            single_database_mode: false,
            time_zone: String::new(),
            keep_alive_secs: String::new(),
            keep_alive_query: String::new(),
            auto_disconnect_secs: String::new(),
            startup_script: String::new(),
            auto_sync: introspection.auto_sync,
            load_sources: introspection.load_sources,
            warn_outdated: introspection.warn_outdated,
            level: introspection.level,
            external_data_file: String::new(),
            ssl_mode: SslMode::default(),
            ca_file: String::new(),
            cert_file: String::new(),
            key_file: String::new(),
            key_password_env: String::new(),
            key_password: String::new(),
            clear_key_password: false,
            schema_pattern: String::new(),
            object_filter: String::new(),
            properties: String::new(),
            use_ssh: false,
            ssh_existing: String::new(),
            ssh_new: false,
            ssh_local_port: String::new(),
            ssh_new_name: String::new(),
            ssh_host: String::new(),
            ssh_port: String::new(),
            ssh_user: String::new(),
            ssh_auth: SshAuth::default(),
            ssh_key_file: String::new(),
            ssh_password_env: String::new(),
            ssh_password: String::new(),
            ssh_passphrase_env: String::new(),
            ssh_passphrase: String::new(),
            ssh_host_key: HostKeyPolicy::default(),
            ssh_known_hosts_file: String::new(),
        }
    }
}

fn optional(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Empty means "not set"; otherwise a positive whole number of seconds.
fn seconds(label: &str, text: &str) -> Result<Option<u32>, String> {
    match optional(text) {
        None => Ok(None),
        Some(t) => match t.parse::<u32>() {
            Ok(n) if n > 0 => Ok(Some(n)),
            _ => Err(format!("{label} `{t}` must be a whole number of seconds above 0.")),
        },
    }
}

/// `name=value` lines. Blank lines and `#` comments are skipped.
fn properties(text: &str) -> Result<Properties, String> {
    let mut map = Properties::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("Advanced, line {}: expected name=value.", i + 1))?;
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("Advanced, line {}: the property name is empty.", i + 1));
        }
        map.insert(key.to_owned(), value.trim().to_owned());
    }
    Ok(map)
}

fn color(text: &str) -> Result<Option<String>, String> {
    let Some(c) = optional(text) else { return Ok(None) };
    let hex = c.strip_prefix('#').unwrap_or_default();
    if hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(Some(c.to_lowercase()))
    } else {
        Err(format!("Color `{c}` must look like #1a2b3c."))
    }
}

impl Draft {
    /// Populate the shared form without resolving any saved credentials.
    pub fn from_config(config: &DataSourceConfig) -> Self {
        Self {
            name: config.name.clone(),
            comment: config.comment.clone().unwrap_or_default(),
            color: config.color.clone().unwrap_or_default(),
            connection_type: config.connection_type.clone().unwrap_or_default(),
            driver: config
                .driver
                .clone()
                .or_else(|| {
                    rowcraft_drivers::default_for_dialect(&config.dialect).map(|d| d.id.to_owned())
                })
                .unwrap_or_default(),
            dialect: config.dialect.clone(),
            host: config.host.clone().unwrap_or_default(),
            port: config.port.map(|v| v.to_string()).unwrap_or_default(),
            database: config.database.clone().unwrap_or_default(),
            user: config.user.clone().unwrap_or_default(),
            password_env: config.password_env.clone().unwrap_or_default(),
            url: config.url.clone().unwrap_or_default(),
            auth: config.auth,
            read_only: config.read_only,
            transaction: config.options.transaction,
            isolation: config.options.isolation,
            switch_schema: config.options.switch_schema,
            session_control: config.options.session_control,
            single_database_mode: config.options.single_database_mode,
            time_zone: config.options.time_zone.clone().unwrap_or_default(),
            keep_alive_secs: config
                .options
                .keep_alive_secs
                .map(|v| v.to_string())
                .unwrap_or_default(),
            keep_alive_query: config.options.keep_alive_query.clone().unwrap_or_default(),
            auto_disconnect_secs: config
                .options
                .auto_disconnect_secs
                .map(|v| v.to_string())
                .unwrap_or_default(),
            startup_script: config.options.startup_script.clone().unwrap_or_default(),
            auto_sync: config.introspection.auto_sync,
            load_sources: config.introspection.load_sources,
            warn_outdated: config.introspection.warn_outdated,
            level: config.introspection.level,
            external_data_file: config.introspection.external_data_file.clone().unwrap_or_default(),
            ssl_mode: config.ssl_mode,
            ca_file: config.ssl_files.ca_file.clone().unwrap_or_default(),
            cert_file: config.ssl_files.cert_file.clone().unwrap_or_default(),
            key_file: config.ssl_files.key_file.clone().unwrap_or_default(),
            key_password_env: config.ssl_files.key_password_env.clone().unwrap_or_default(),
            schema_pattern: config.schema_pattern.clone().unwrap_or_default(),
            object_filter: config.object_filter.clone().unwrap_or_default(),
            properties: config
                .properties
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join("\n"),
            use_ssh: config.ssh_tunnel.is_some(),
            ssh_existing: config.ssh_tunnel.clone().unwrap_or_default(),
            ssh_local_port: config.ssh_local_port.map(|v| v.to_string()).unwrap_or_default(),
            ..Self::default()
        }
    }

    pub fn preserve_saved_secrets(
        &self,
        original: &DataSourceConfig,
        config: &mut DataSourceConfig,
    ) {
        if self.auth == AuthType::UserPassword && self.password.is_empty() && !self.clear_password {
            config.password_keychain = original.password_keychain.clone();
        } else if self.clear_password && self.password.is_empty() {
            config.password_env = None;
        }
        if self.key_password.is_empty() && !self.clear_key_password {
            config.ssl_files.key_password = original.ssl_files.key_password.clone();
        } else if self.clear_key_password && self.key_password.is_empty() {
            config.ssl_files.key_password_env = None;
        }
    }

    /// The values the form starts with.
    pub fn initial() -> Self {
        let mut draft = Self { ssl_mode: SslMode::Auto, ..Self::default() };
        draft.select_driver("postgresql");
        draft
    }

    /// Picks a catalog driver: sets the dialect and, for a network database with no host yet,
    /// `localhost`. Text the user typed is never overwritten. Unknown ids are ignored.
    pub fn select_driver(&mut self, id: &str) {
        let Some(driver) = rowcraft_drivers::find(id) else { return };
        self.driver = driver.id.to_owned();
        self.dialect = driver.dialect.unwrap_or_default().to_owned();
        if driver.target == Target::Network && self.host.trim().is_empty() {
            self.host = "localhost".into();
        }
    }

    /// Checks the form and returns the data source, or a message for the user.
    /// The SSH configuration typed into the "new" fields.
    pub fn new_ssh_config(&self) -> Result<SshConfig, String> {
        let port = match optional(&self.ssh_port) {
            None => None,
            Some(p) => match p.parse::<u16>() {
                Ok(n) if n > 0 => Some(n),
                _ => return Err(format!("SSH port `{p}` must be a number from 1 to 65535.")),
            },
        };
        let config = SshConfig {
            name: self.ssh_new_name.trim().to_owned(),
            host: self.ssh_host.trim().to_owned(),
            port,
            user: self.ssh_user.trim().to_owned(),
            auth: self.ssh_auth,
            key_file: optional(&self.ssh_key_file),
            password_env: if self.ssh_password.is_empty() {
                optional(&self.ssh_password_env)
            } else {
                None
            },
            password: rowcraft_config::passwords::StoredSecret::entered(
                if self.ssh_auth == SshAuth::Password { &self.ssh_password } else { "" },
            ),
            passphrase_env: if self.ssh_passphrase.is_empty() {
                optional(&self.ssh_passphrase_env)
            } else {
                None
            },
            passphrase: rowcraft_config::passwords::StoredSecret::entered(
                if self.ssh_auth == SshAuth::KeyPair { &self.ssh_passphrase } else { "" },
            ),
            host_key: self.ssh_host_key,
            known_hosts_file: optional(&self.ssh_known_hosts_file),
        };
        config.validate()?;
        Ok(config)
    }

    /// [`Draft::validate_full`] for forms that do not use an SSH tunnel.
    pub fn validate(
        &self,
        known_dialects: &[&str],
        existing: &[DataSourceConfig],
    ) -> Result<DataSourceConfig, String> {
        self.validate_full(known_dialects, existing, &[]).map(|v| v.config)
    }

    /// Checks the whole form. `ssh_names` are the saved SSH configurations a data source may use.
    pub fn validate_full(
        &self,
        known_dialects: &[&str],
        existing: &[DataSourceConfig],
        ssh_names: &[String],
    ) -> Result<Validated, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Name is required.".into());
        }
        if existing.iter().any(|d| d.name == name) {
            return Err(format!("A data source named `{name}` already exists."));
        }

        let driver = match self.driver.trim() {
            "" => None,
            id => {
                Some(rowcraft_drivers::find(id).ok_or_else(|| format!("Unknown driver `{id}`."))?)
            }
        };
        let dialect = match driver {
            Some(d) => d.dialect.ok_or_else(|| {
                format!(
                    "{} has no pure-Rust driver yet; pick a database marked as supported.",
                    d.name
                )
            })?,
            None => {
                let typed = self.dialect.trim();
                known_dialects.iter().copied().find(|k| *k == typed).ok_or_else(|| {
                    format!("Unknown dialect `{typed}`. Available: {}.", known_dialects.join(", "))
                })?
            }
        };
        if !known_dialects.contains(&dialect) {
            return Err(format!("The `{dialect}` dialect is not part of this build."));
        }
        let file_based = driver.is_some_and(|d| d.target == Target::File);

        let port = match optional(&self.port) {
            None => None,
            Some(p) => match p.parse::<u16>() {
                Ok(n) if n > 0 => Some(n),
                _ => return Err(format!("Port `{p}` must be a number from 1 to 65535.")),
            },
        };
        if file_based && optional(&self.database).is_none() && optional(&self.url).is_none() {
            return Err("This is a file database: enter its path in Database.".into());
        }

        let url = optional(&self.url);
        if let Some(u) = &url {
            parse_url(u).map_err(|e| format!("URL: {e}"))?;
        }

        let keep_alive_secs = seconds("Keep-alive interval", &self.keep_alive_secs)?;
        let auto_disconnect_secs = seconds("Auto-disconnect time", &self.auto_disconnect_secs)?;

        if optional(&self.cert_file).is_some() != optional(&self.key_file).is_some() {
            return Err(
                "SSL: a client certificate needs a client key file, and the other way round."
                    .into(),
            );
        }

        let cfg = DataSourceConfig {
            name: name.to_owned(),
            dialect: dialect.to_owned(),
            host: if file_based { None } else { optional(&self.host) },
            port: if file_based { None } else { port },
            database: optional(&self.database),
            user: optional(&self.user),
            password_env: if self.password.is_empty() {
                optional(&self.password_env)
            } else {
                None
            },
            password_keychain: None,
            ssl_mode: self.ssl_mode,
            read_only: self.read_only,
            driver: driver.map(|d| d.id.to_owned()),
            comment: optional(&self.comment),
            color: color(&self.color)?,
            connection_type: optional(&self.connection_type),
            url,
            auth: self.auth,
            options: ConnectionOptions {
                transaction: self.transaction,
                isolation: self.isolation,
                switch_schema: self.switch_schema,
                time_zone: optional(&self.time_zone),
                keep_alive_secs,
                keep_alive_query: optional(&self.keep_alive_query),
                auto_disconnect_secs,
                single_database_mode: self.single_database_mode,
                session_control: self.session_control,
                startup_script: optional(&self.startup_script),
            },
            introspection: IntrospectionOptions {
                auto_sync: self.auto_sync,
                load_sources: self.load_sources,
                warn_outdated: self.warn_outdated,
                level: self.level,
                external_data_file: optional(&self.external_data_file),
            },
            ssl_files: SslFiles {
                ca_file: optional(&self.ca_file),
                cert_file: optional(&self.cert_file),
                key_file: optional(&self.key_file),
                key_password_env: if self.key_password.is_empty() {
                    optional(&self.key_password_env)
                } else {
                    None
                },
                key_password: rowcraft_config::passwords::StoredSecret::entered(&self.key_password),
            },
            schema_pattern: optional(&self.schema_pattern),
            object_filter: optional(&self.object_filter),
            properties: properties(&self.properties)?,
            ssh_tunnel: None,
            ssh_local_port: None,
            password: (self.auth == AuthType::UserPassword && !self.password.is_empty())
                .then(|| self.password.clone()),
        };
        // The Schemas tab fields are checked with the same parsers the engine uses.
        let mut cfg = cfg;
        cfg.parsed_schema_pattern().map_err(|e| e.to_string())?;
        cfg.parsed_object_filter().map_err(|e| e.to_string())?;

        let mut new_ssh = None;
        if self.use_ssh {
            if self.ssh_new {
                let config = self.new_ssh_config()?;
                if ssh_names.contains(&config.name) {
                    return Err(format!(
                        "An SSH configuration named `{}` already exists.",
                        config.name
                    ));
                }
                cfg.ssh_tunnel = Some(config.name.clone());
                new_ssh = Some(config);
            } else {
                let chosen = self.ssh_existing.trim();
                if chosen.is_empty() {
                    return Err("SSH: choose an SSH configuration, or create a new one.".into());
                }
                if !ssh_names.iter().any(|n| n == chosen) {
                    return Err(format!("SSH: there is no SSH configuration named `{chosen}`."));
                }
                cfg.ssh_tunnel = Some(chosen.to_owned());
            }
            cfg.ssh_local_port = match optional(&self.ssh_local_port) {
                None => None,
                Some(p) => match p.parse::<u16>() {
                    Ok(n) if n > 0 => Some(n),
                    _ => {
                        return Err(format!(
                            "Forced local port `{p}` must be a number from 1 to 65535."
                        ));
                    }
                },
            };
            let nodes = cfg.host.as_deref().map_or(0, |h| h.split(',').count());
            if let (Some(port), true) = (cfg.ssh_local_port, nodes > 1) {
                return Err(format!(
                    "SSH: unable to create multiple tunnels ({nodes} hosts) when the local port is forced to {port}."
                ));
            }
        }
        Ok(Validated { config: cfg, new_ssh })
    }
}

/// Saves `ssh` to the SSH configurations file at `path`.
pub fn save_new_ssh(path: &Path, ssh: SshConfig) -> Result<(), String> {
    let mut file = SshFile::load(path).map_err(|e| e.to_string())?;
    file.add(ssh).map_err(|e| e.to_string())?;
    file.save(path).map_err(|e| e.to_string())
}

/// Adds `cfg` to the data sources file at `path` and returns the file's full list afterwards.
/// Re-reads the file first, so a source added by the CLI in the meantime is kept.
pub fn save_new(path: &Path, cfg: DataSourceConfig) -> Result<Vec<DataSourceConfig>, String> {
    let mut file = DataSourceFile::load(path).map_err(|e| e.to_string())?;
    file.add(cfg).map_err(|e| e.to_string())?;
    file.save(path).map_err(|e| e.to_string())?;
    Ok(file.datasources)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIALECTS: &[&str] = &["pg", "mysql", "sqlite"];

    fn filled() -> Draft {
        Draft {
            name: " local ".into(),
            port: "5433".into(),
            database: "app".into(),
            user: "me".into(),
            password_env: "APP_PW".into(),
            ..Draft::initial()
        }
    }

    fn err(d: Draft) -> String {
        d.validate(DIALECTS, &[]).unwrap_err()
    }

    #[test]
    fn editing_can_replace_or_explicitly_clear_saved_passwords() {
        let mut original = filled().validate(DIALECTS, &[]).unwrap();
        original.password_keychain = Some("saved-db".into());
        original.ssl_files.key_password.account = Some("saved-ssl".into());
        original.ssl_files.key_password_env = Some("SSL_PASSWORD".into());
        let mut draft = Draft::from_config(&original);
        let mut config = draft.validate(DIALECTS, &[]).unwrap();
        draft.preserve_saved_secrets(&original, &mut config);
        assert_eq!(config, original);
        draft.clear_password = true;
        draft.clear_key_password = true;
        let mut cleared = draft.validate(DIALECTS, &[]).unwrap();
        draft.preserve_saved_secrets(&original, &mut cleared);
        assert!(cleared.password_keychain.is_none() && cleared.password_env.is_none());
        assert!(
            cleared.ssl_files.key_password.account.is_none()
                && cleared.ssl_files.key_password_env.is_none()
        );
        draft.password = "replacement password".into();
        draft.key_password = "replacement key password".into();
        let mut replaced = draft.validate(DIALECTS, &[]).unwrap();
        draft.preserve_saved_secrets(&original, &mut replaced);
        assert_eq!(replaced.password.as_deref(), Some("replacement password"));
        assert_eq!(
            replaced.ssl_files.key_password.value.as_deref(),
            Some("replacement key password")
        );
        assert!(
            replaced.password_keychain.is_none()
                && replaced.ssl_files.key_password.account.is_none()
        );
    }

    #[test]
    fn direct_password_is_preserved_and_excluded_from_saved_config_and_debug() {
        let draft = Draft { password: " demo-only password ".into(), ..filled() };
        let mut config = draft.validate(DIALECTS, &[]).unwrap();
        assert_eq!(config.password.as_deref(), Some(" demo-only password "));
        assert!(config.password_env.is_none());
        config.resolve_password().unwrap();
        let text = toml::to_string(&config).unwrap();
        assert!(!text.contains("demo-only"));
        assert!(!format!("{draft:?}").contains("demo-only"));
        assert!(!format!("{config:?}").contains("demo-only"));
    }

    #[test]
    fn no_auth_discards_direct_password() {
        let draft = Draft { password: "demo-only".into(), auth: AuthType::NoAuth, ..filled() };
        assert!(draft.validate(DIALECTS, &[]).unwrap().password.is_none());
    }

    #[test]
    fn ssh_and_ssl_secrets_preserve_whitespace_and_never_reach_saved_files_or_debug() {
        let mut draft = filled();
        draft.use_ssh = true;
        draft.ssh_new = true;
        draft.ssh_new_name = "jump".into();
        draft.ssh_host = "localhost".into();
        draft.ssh_user = "demo".into();
        draft.ssh_password = " ssh-demo-only ".into();
        draft.key_password = " ssl-demo-only ".into();
        let valid = draft.validate_full(DIALECTS, &[], &[]).unwrap();
        let ssh = valid.new_ssh.unwrap();
        assert_eq!(ssh.password.resolve().unwrap().as_deref(), Some(" ssh-demo-only "));
        assert_eq!(
            valid.config.ssl_files.key_password.resolve().unwrap().as_deref(),
            Some(" ssl-demo-only ")
        );
        for text in [
            toml::to_string(&ssh).unwrap(),
            toml::to_string(&valid.config).unwrap(),
            format!("{draft:?} {ssh:?} {:?}", valid.config.ssl_files),
        ] {
            assert!(!text.contains("demo-only"));
        }
        draft.ssh_auth = SshAuth::KeyPair;
        draft.ssh_key_file = "~/.ssh/id_ed25519".into();
        draft.ssh_passphrase = " key-demo-only ".into();
        let ssh = draft.new_ssh_config().unwrap();
        assert_eq!(ssh.passphrase.resolve().unwrap().as_deref(), Some(" key-demo-only "));
        assert!(ssh.password.value.is_none());
        assert!(!toml::to_string(&ssh).unwrap().contains("demo-only"));
    }

    #[test]
    fn a_complete_form_becomes_a_data_source() {
        let ds = filled().validate(DIALECTS, &[]).unwrap();
        assert_eq!(ds.name, "local");
        assert_eq!(
            (ds.dialect.as_str(), ds.driver.as_deref(), ds.host.as_deref(), ds.port),
            ("pg", Some("postgresql"), Some("localhost"), Some(5433))
        );
        assert_eq!(ds.password_env.as_deref(), Some("APP_PW"));
        assert_eq!(ds.ssl_mode, SslMode::Auto);
        assert!(ds.password.is_none());
    }

    #[test]
    fn an_untouched_form_saves_only_the_essentials() {
        // Defaults must not leak into the file as noise: the saved TOML names only what was set.
        let ds = Draft { name: "x".into(), ..Draft::initial() }.validate(DIALECTS, &[]).unwrap();
        let text = toml::to_string_pretty(&DataSourceFile { datasources: vec![ds] }).unwrap();
        for noise in ["options", "introspection", "ssl_files", "properties", "comment", "url"] {
            assert!(!text.contains(noise), "{noise} in\n{text}");
        }
    }

    #[test]
    fn every_option_reaches_the_config() {
        let draft = Draft {
            comment: "replica".into(),
            color: "#AABBCC".into(),
            connection_type: "production".into(),
            url: "postgresql://h:1/d".into(),
            auth: AuthType::NoAuth,
            read_only: true,
            transaction: TransactionMode::Manual,
            isolation: Isolation::ReadCommitted,
            switch_schema: SwitchSchema::Manual,
            session_control: SessionControl::Dedicated,
            single_database_mode: true,
            time_zone: "UTC".into(),
            keep_alive_secs: "60".into(),
            keep_alive_query: "select 1".into(),
            auto_disconnect_secs: "300".into(),
            startup_script: "set x = 1;".into(),
            auto_sync: false,
            load_sources: LoadSources::None,
            warn_outdated: false,
            level: IntrospectionLevel::Names,
            external_data_file: "v.xml".into(),
            ssl_mode: SslMode::VerifyCa,
            ca_file: "ca.pem".into(),
            cert_file: "c.pem".into(),
            key_file: "k.pem".into(),
            key_password_env: "KPW".into(),
            schema_pattern: "*:public".into(),
            object_filter: "table:-tmp.*".into(),
            properties: "application_name = rowcraft\n# note\n\nsearch=1".into(),
            ..filled()
        };
        let ds = draft.validate(DIALECTS, &[]).unwrap();
        assert_eq!(
            (ds.comment.as_deref(), ds.color.as_deref()),
            (Some("replica"), Some("#aabbcc"))
        );
        assert_eq!(ds.connection_type.as_deref(), Some("production"));
        assert_eq!(
            (ds.auth, ds.read_only, ds.ssl_mode),
            (AuthType::NoAuth, true, SslMode::VerifyCa)
        );
        assert_eq!(ds.options.transaction, TransactionMode::Manual);
        assert_eq!(ds.options.isolation, Isolation::ReadCommitted);
        assert_eq!(ds.options.keep_alive_secs, Some(60));
        assert_eq!(ds.options.auto_disconnect_secs, Some(300));
        assert_eq!(ds.options.startup_script.as_deref(), Some("set x = 1;"));
        assert!(ds.options.single_database_mode);
        assert!(!ds.introspection.auto_sync);
        assert_eq!(ds.introspection.load_sources, LoadSources::None);
        assert_eq!(ds.ssl_files.key_password_env.as_deref(), Some("KPW"));
        assert_eq!(ds.schema_pattern.as_deref(), Some("*:public"));
        assert_eq!(
            ds.properties.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect::<Vec<_>>(),
            [("application_name", "rowcraft"), ("search", "1")]
        );
    }

    #[test]
    fn each_bad_field_gets_its_own_message() {
        assert!(err(Draft { name: "  ".into(), ..filled() }).contains("Name"));
        for bad in ["abc", "0", "70000", "-1", "54 32"] {
            assert!(err(Draft { port: bad.into(), ..filled() }).contains("Port"), "{bad}");
        }
        assert!(err(Draft { url: "nonsense".into(), ..filled() }).starts_with("URL:"));
        assert!(err(Draft { url: "pg://u:pw@h/d".into(), ..filled() }).contains("password"));
        assert!(err(Draft { keep_alive_secs: "0".into(), ..filled() }).contains("Keep-alive"));
        assert!(
            err(Draft { auto_disconnect_secs: "soon".into(), ..filled() })
                .contains("Auto-disconnect")
        );
        assert!(err(Draft { color: "red".into(), ..filled() }).contains("Color"));
        assert!(err(Draft { color: "#12345".into(), ..filled() }).contains("Color"));
        assert!(
            err(Draft { schema_pattern: "*:{(}".into(), ..filled() }).contains("schema pattern")
        );
        assert!(
            err(Draft { object_filter: "table:(".into(), ..filled() }).contains("object filter")
        );
        assert!(err(Draft { properties: "novalue".into(), ..filled() }).contains("line 1"));
        assert!(err(Draft { properties: "a=1\n=2".into(), ..filled() }).contains("line 2"));
    }

    #[test]
    fn a_client_certificate_and_its_key_come_together() {
        assert!(err(Draft { cert_file: "c.pem".into(), ..filled() }).contains("client key"));
        assert!(err(Draft { key_file: "k.pem".into(), ..filled() }).contains("client key"));
        let both = Draft { cert_file: "c.pem".into(), key_file: "k.pem".into(), ..filled() };
        assert!(both.validate(DIALECTS, &[]).is_ok());
    }

    #[test]
    fn a_driver_decides_the_dialect_and_missing_build_features_are_refused() {
        let mut d = filled();
        d.select_driver("mysql");
        assert_eq!(d.validate(DIALECTS, &[]).unwrap().dialect, "mysql");

        d.select_driver("athena");
        assert!(err(d.clone()).contains("not part of this build"));
        assert_eq!(d.validate(&["jdbc"], &[]).unwrap().dialect, "jdbc");

        // A driver whose dialect this build lacks.
        d.select_driver("oracle");
        assert!(err(d.clone()).contains("not part of this build"));

        // Without a driver, the typed dialect is checked.
        let typed = Draft { driver: String::new(), dialect: "nope".into(), ..filled() };
        assert!(err(typed).contains("Unknown dialect `nope`"));
        let typed = Draft { driver: String::new(), dialect: "mysql".into(), ..filled() };
        assert_eq!(typed.validate(DIALECTS, &[]).unwrap().dialect, "mysql");
        assert!(err(Draft { driver: "ghost".into(), ..filled() }).contains("Unknown driver"));
    }

    #[test]
    fn picking_a_driver_never_overwrites_typed_text() {
        let mut d = Draft { host: "db.internal".into(), port: "6000".into(), ..Draft::default() };
        d.select_driver("postgresql");
        assert_eq!(
            (d.host.as_str(), d.port.as_str(), d.dialect.as_str()),
            ("db.internal", "6000", "pg")
        );
        let mut empty = Draft::default();
        empty.select_driver("postgresql");
        assert_eq!(empty.host, "localhost");
        empty.select_driver("no-such-driver");
        assert_eq!(empty.driver, "postgresql", "an unknown id leaves the selection alone");
    }

    #[test]
    fn file_databases_need_a_path_and_drop_host_and_port() {
        let mut d = Draft { name: "lite".into(), ..Draft::default() };
        d.select_driver("sqlite");
        assert!(d.host.is_empty(), "no host for a file database");
        assert!(err(d.clone()).contains("path"));
        d.database = "/tmp/app.db".into();
        d.host = "ignored".into();
        d.port = "1".into();
        let ds = d.validate(DIALECTS, &[]).unwrap();
        assert_eq!((ds.host, ds.port, ds.database.as_deref()), (None, None, Some("/tmp/app.db")));
    }

    #[test]
    fn duplicate_names_are_refused() {
        let first = filled().validate(DIALECTS, &[]).unwrap();
        let err = filled().validate(DIALECTS, &[first]).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
    }

    #[test]
    fn saving_appends_to_the_file_and_keeps_what_was_there() {
        let dir = std::env::temp_dir().join(format!("rowcraft-form-{}", std::process::id()));
        let path = dir.join("datasources.toml");
        let _ = std::fs::remove_dir_all(&dir);

        let a = filled().validate(DIALECTS, &[]).unwrap();
        assert_eq!(save_new(&path, a.clone()).unwrap().len(), 1);

        let b = Draft { name: "second".into(), ..filled() }.validate(DIALECTS, &[]).unwrap();
        let all = save_new(&path, b).unwrap();
        assert_eq!(all.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["local", "second"]);

        // Saved text names the env var and never contains a password field.
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("password_env = \"APP_PW\"") && !text.contains("password ="));

        // Another process may have added the same name since the form was validated.
        assert!(save_new(&path, a).unwrap_err().contains("already exists"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- SSH ------------------------------------------------------------------------------

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn with_ssh() -> Draft {
        Draft { use_ssh: true, ssh_existing: "jump".into(), ..filled() }
    }

    #[test]
    fn choosing_an_existing_ssh_configuration() {
        let v = with_ssh().validate_full(DIALECTS, &[], &names(&["jump", "other"])).unwrap();
        assert_eq!(v.config.ssh_tunnel.as_deref(), Some("jump"));
        assert_eq!(v.config.ssh_local_port, None);
        assert!(v.new_ssh.is_none());

        let forced = Draft { ssh_local_port: " 15432 ".into(), ..with_ssh() };
        let v = forced.validate_full(DIALECTS, &[], &names(&["jump"])).unwrap();
        assert_eq!(v.config.ssh_local_port, Some(15432));
    }

    #[test]
    fn a_form_without_the_tunnel_ignores_the_ssh_fields() {
        let d = Draft {
            use_ssh: false,
            ssh_existing: "jump".into(),
            ssh_local_port: "junk".into(),
            ..filled()
        };
        let v = d.validate_full(DIALECTS, &[], &names(&["jump"])).unwrap();
        assert_eq!((v.config.ssh_tunnel, v.config.ssh_local_port), (None, None));
    }

    #[test]
    fn ssh_choices_that_cannot_work_are_explained() {
        let known = names(&["jump"]);
        let err = |d: Draft| d.validate_full(DIALECTS, &[], &known).unwrap_err();
        assert!(
            err(Draft { ssh_existing: String::new(), ..with_ssh() })
                .contains("choose an SSH configuration")
        );
        assert!(
            err(Draft { ssh_existing: "ghost".into(), ..with_ssh() })
                .contains("no SSH configuration named `ghost`")
        );
        assert!(
            err(Draft { ssh_local_port: "0".into(), ..with_ssh() }).contains("Forced local port")
        );
        assert!(
            err(Draft { ssh_local_port: "x".into(), ..with_ssh() }).contains("Forced local port")
        );
        // A forced port cannot serve several cluster endpoints.
        let cluster =
            Draft { host: "a:1,b:2".into(), ssh_local_port: "15432".into(), ..with_ssh() };
        let message = err(cluster);
        assert!(
            message.contains("multiple tunnels (2 hosts)") && message.contains("15432"),
            "{message}"
        );
        // Without a forced port a cluster is fine.
        let cluster = Draft { host: "a:1,b:2".into(), ..with_ssh() };
        assert!(cluster.validate_full(DIALECTS, &[], &known).is_ok());
    }

    #[test]
    fn a_new_ssh_configuration_is_validated_and_returned_for_saving() {
        let fresh = Draft {
            use_ssh: true,
            ssh_new: true,
            ssh_new_name: " bastion ".into(),
            ssh_host: "gw.example.com".into(),
            ssh_port: "2222".into(),
            ssh_user: "me".into(),
            ssh_auth: SshAuth::KeyPair,
            ssh_key_file: "~/.ssh/id_ed25519".into(),
            ssh_passphrase_env: "KEY_PW".into(),
            ssh_host_key: HostKeyPolicy::AcceptNew,
            ..filled()
        };
        let v = fresh.validate_full(DIALECTS, &[], &names(&["jump"])).unwrap();
        let ssh = v.new_ssh.expect("a new configuration");
        assert_eq!(v.config.ssh_tunnel.as_deref(), Some("bastion"));
        assert_eq!(
            (ssh.name.as_str(), ssh.host.as_str(), ssh.port, ssh.user.as_str()),
            ("bastion", "gw.example.com", Some(2222), "me")
        );
        assert_eq!((ssh.auth, ssh.host_key), (SshAuth::KeyPair, HostKeyPolicy::AcceptNew));
        assert_eq!(ssh.passphrase_env.as_deref(), Some("KEY_PW"));

        let err = |d: Draft| d.validate_full(DIALECTS, &[], &names(&["jump"])).unwrap_err();
        assert!(
            err(Draft { ssh_new_name: "jump".into(), ..fresh.clone() }).contains("already exists")
        );
        assert!(err(Draft { ssh_host: String::new(), ..fresh.clone() }).contains("host"));
        assert!(err(Draft { ssh_user: String::new(), ..fresh.clone() }).contains("user"));
        assert!(err(Draft { ssh_port: "99999".into(), ..fresh.clone() }).contains("SSH port"));
        assert!(err(Draft { ssh_key_file: String::new(), ..fresh }).contains("key file"));
    }

    #[test]
    fn a_secret_is_never_a_field_of_the_ssh_configuration() {
        // Only the *names* of environment variables are kept.
        let fresh = Draft {
            use_ssh: true,
            ssh_new: true,
            ssh_new_name: "b".into(),
            ssh_host: "h".into(),
            ssh_user: "u".into(),
            ssh_password_env: "SSH_PW".into(),
            ..filled()
        };
        let ssh = fresh.validate_full(DIALECTS, &[], &[]).unwrap().new_ssh.unwrap();
        let text = toml::to_string_pretty(&SshFile { configs: vec![ssh] }).unwrap();
        assert!(
            text.contains("password_env = \"SSH_PW\"") && !text.contains("password ="),
            "{text}"
        );
    }

    #[test]
    fn saving_a_new_ssh_configuration_appends_to_ssh_toml() {
        let dir = std::env::temp_dir().join(format!("rowcraft-form-ssh-{}", std::process::id()));
        let path = dir.join("ssh.toml");
        let _ = std::fs::remove_dir_all(&dir);
        let ssh = |name: &str| SshConfig {
            name: name.into(),
            host: "h".into(),
            user: "u".into(),
            ..Default::default()
        };
        save_new_ssh(&path, ssh("a")).unwrap();
        save_new_ssh(&path, ssh("b")).unwrap();
        assert!(save_new_ssh(&path, ssh("a")).unwrap_err().contains("already exists"));
        let saved = SshFile::load(&path).unwrap();
        assert_eq!(saved.configs.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
