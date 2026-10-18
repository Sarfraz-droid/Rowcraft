//! The contract every database implements.

mod registry;
mod split;

pub use registry::Registry;
pub use split::split_statements;

use async_trait::async_trait;
use dg_config::DataSourceConfig;
use dg_model::Catalog;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} is not implemented yet")]
    NotImplemented(&'static str),
    #[error("unknown dialect `{0}`")]
    UnknownDialect(String),
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("query failed: {0}")]
    Query(String),
}

/// Stable identifier used in config files and the CLI, e.g. `"pg"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DialectId(pub &'static str);

impl std::fmt::Display for DialectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Decides which console language and explorer shape the UI uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialectFamily {
    Sql,
    Document,
    KeyValue,
    WideColumn,
    Graph,
    Api,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub transactions: bool,
    pub cancel: bool,
    pub editable_results: bool,
    pub explain: bool,
}

/// A single cell value. TODO(M1): full type system (decimals, temporals, arrays, geometry, …).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
}

/// Result of one statement (for a script, the last statement that produced one).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    /// Rows changed by the statement, when the database reports it.
    pub rows_affected: Option<u64>,
    /// More rows were available than the caller's limit; `rows` holds the first ones.
    pub truncated: bool,
}

#[async_trait]
pub trait Dialect: Send + Sync {
    fn id(&self) -> DialectId;
    fn display_name(&self) -> &'static str;
    fn family(&self) -> DialectFamily;
    fn default_port(&self) -> Option<u16> {
        None
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    /// Whether `connect` works. False for a dialect that is only registered so far; the CLI and
    /// the GUI use it to tell users which databases are usable today.
    fn connects(&self) -> bool {
        false
    }
    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>>;

    /// Reads the catalog (schemas, tables, columns, indexes, constraints, …) through `conn`.
    async fn introspect(
        &self,
        _conn: &mut dyn Connection,
        _scope: &IntrospectScope,
    ) -> Result<Catalog> {
        Err(Error::NotImplemented("introspection"))
    }
    // TODO(M1): map_type, ddl, explain, sql_flavor (see SPEC.md §3.3); incremental introspection.
}

/// Which part of the database to introspect. System schemas are always skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntrospectScope {
    /// Only these schemas; `None` means every user schema.
    pub schemas: Option<Vec<String>>,
}

#[async_trait]
pub trait Connection: Send + std::fmt::Debug {
    /// Runs `statement`, keeping at most `max_rows` rows in memory.
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet>;

    async fn query(&mut self, statement: &str) -> Result<ResultSet> {
        self.query_limited(statement, usize::MAX).await
    }
    // TODO(M1): query_stream (true paging), execute with params, cancel_handle, begin/commit/rollback.
}
