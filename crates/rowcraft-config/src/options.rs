//! The per-data-source connection options, grouped as its tabs are: Options
//! (connection and introspection), SSH/SSL and Advanced. Labels follow `DatabaseBundle.properties`.
//!
//! Every field has a default, so older `datasources.toml` files keep loading. Which options the
//! engine acts on today is tracked in docs/data-source-options.md.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// "Authentication:" on the General tab.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthType {
    /// "User & Password"
    #[default]
    UserPassword,
    /// "No auth"
    NoAuth,
    /// "Windows credentials" (integrated authentication)
    WindowsCredentials,
}

/// "Transaction control:" — whether each statement commits by itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransactionMode {
    #[default]
    Auto,
    Manual,
}

/// Isolation level offered next to the transaction mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Isolation {
    #[default]
    DatabaseDefault,
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

/// "Switch schema:"
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SwitchSchema {
    #[default]
    Automatic,
    Manual,
    Disabled,
}

/// "Session control:"
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionControl {
    /// "Single shared session"
    #[default]
    Single,
    /// "Dedicated sessions"
    Dedicated,
    /// "Two sessions: shared and introspection"
    SeparateIntrospection,
}

/// Options tab, "Connection" section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectionOptions {
    pub transaction: TransactionMode,
    pub isolation: Isolation,
    pub switch_schema: SwitchSchema,
    pub time_zone: Option<String>,
    /// "Run keep-alive query each N sec."
    pub keep_alive_secs: Option<u32>,
    pub keep_alive_query: Option<String>,
    /// "Auto-disconnect after N sec."
    pub auto_disconnect_secs: Option<u32>,
    /// "Single database mode"
    pub single_database_mode: bool,
    pub session_control: SessionControl,
    /// Statements run right after connecting.
    pub startup_script: Option<String>,
}

/// "Load sources for:"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoadSources {
    AllSchemas,
    /// Default for new data sources.
    AllExclSystem,
    None,
}

/// Introspection level: how much of each object is read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IntrospectionLevel {
    /// "Auto select"
    #[default]
    Auto,
    /// "Level 1: Names only"
    Names,
    /// "Level 2: Skip sources"
    SkipSources,
    /// "Level 3: Everything"
    Everything,
}

/// Options tab, "Introspection" section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IntrospectionOptions {
    /// "Auto sync"
    pub auto_sync: bool,
    pub load_sources: LoadSources,
    /// "Warn when editing outdated DDL"
    pub warn_outdated: bool,
    pub level: IntrospectionLevel,
    /// "Virtual objects and attributes:" file with extra metadata.
    pub external_data_file: Option<String>,
}

impl Default for IntrospectionOptions {
    fn default() -> Self {
        Self {
            auto_sync: true,
            load_sources: LoadSources::AllExclSystem,
            warn_outdated: true,
            level: IntrospectionLevel::Auto,
            external_data_file: None,
        }
    }
}

/// SSH/SSL tab, "Use SSL" section. The mode itself is [`crate::SslMode`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SslFiles {
    /// "CA file:"
    pub ca_file: Option<String>,
    /// "Client certificate file:"
    pub cert_file: Option<String>,
    /// "Client key file:"
    pub key_file: Option<String>,
    /// Name of the environment variable holding the "Client key password".
    pub key_password_env: Option<String>,
    #[serde(skip_serializing_if = "is_default")]
    pub key_password: crate::passwords::StoredSecret,
}

/// Advanced tab: driver properties (Name / Value table).
pub type Properties = BTreeMap<String, String>;

pub(crate) fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}
