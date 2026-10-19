//! SQLite dialect (tier A, see SPEC.md §5), on `rusqlite` with the bundled SQLite.
//!
//! The database is a file named in `database` (`:memory:` works too). rusqlite is blocking, so
//! the connection lives behind a mutex and every call runs on the blocking pool.

mod introspect;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rowcraft_config::DataSourceConfig;
use rowcraft_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result,
    ResultSet, Value,
};
use rowcraft_model::Catalog;
use rusqlite::{OpenFlags, fallible_iterator::FallibleIterator, types::ValueRef};

#[derive(Debug, Default, Clone, Copy)]
pub struct SqliteDialect;

#[async_trait]
impl Dialect for SqliteDialect {
    fn id(&self) -> DialectId {
        DialectId("sqlite")
    }

    fn display_name(&self) -> &'static str {
        "SQLite"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Sql
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { transactions: true, ..Capabilities::default() }
    }

    fn connects(&self) -> bool {
        true
    }

    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        let path = cfg
            .database
            .as_deref()
            .filter(|p| !p.trim().is_empty())
            .ok_or_else(|| Error::Connect("SQLite needs a database file path".into()))?;
        let path = expand_home(path);
        let flags = if cfg.read_only {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
        } | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open_with_flags(&path, flags)?;
            // Opening is lazy; force the header read so a bad file fails here.
            conn.query_row("select count(*) from sqlite_master", [], |_| Ok(()))?;
            Ok::<_, rusqlite::Error>(conn)
        })
        .await
        .map_err(|e| Error::Connect(e.to_string()))?
        .map_err(|e| Error::Connect(e.to_string()))?;
        Ok(Box::new(SqliteConnection { inner: Arc::new(Mutex::new(conn)) }))
    }

    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        scope: &IntrospectScope,
    ) -> Result<Catalog> {
        introspect::introspect(conn, scope).await
    }
}

fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => format!("{}/{rest}", home.to_string_lossy()),
        _ => path.to_owned(),
    }
}

#[derive(Debug)]
struct SqliteConnection {
    inner: Arc<Mutex<rusqlite::Connection>>,
}

#[async_trait]
impl Connection for SqliteConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let inner = Arc::clone(&self.inner);
        let sql = statement.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = inner.lock().map_err(|_| Error::Query("connection poisoned".into()))?;
            run_script(&conn, &sql, max_rows)
        })
        .await
        .map_err(|e| Error::Query(e.to_string()))?
    }
}

/// Runs every statement of `sql`; like the pg dialect, the last one that produced columns (or
/// else the last one) is returned.
fn run_script(conn: &rusqlite::Connection, sql: &str, max_rows: usize) -> Result<ResultSet> {
    let mut last = ResultSet::default();
    let mut batch = rusqlite::Batch::new(conn, sql);
    while let Some(mut stmt) = batch.next().map_err(query_err)? {
        let columns: Vec<String> = stmt.column_names().iter().map(|c| (*c).to_owned()).collect();
        if columns.is_empty() {
            let n = stmt.execute([]).map_err(query_err)?;
            last = ResultSet { rows_affected: Some(n as u64), ..ResultSet::default() };
            continue;
        }
        let width = columns.len();
        let mut current = ResultSet { columns, ..ResultSet::default() };
        let mut rows = stmt.query([]).map_err(query_err)?;
        while let Some(row) = rows.next().map_err(query_err)? {
            if current.rows.len() >= max_rows {
                current.truncated = true;
                break;
            }
            let cells = (0..width)
                .map(|i| row.get_ref(i).map(cell).map_err(query_err))
                .collect::<Result<Vec<_>>>()?;
            current.rows.push(cells);
        }
        last = current;
    }
    Ok(last)
}

fn cell(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::Int(i),
        ValueRef::Real(f) => Value::Float(f),
        ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Value::Bytes(b.to_vec()),
    }
}

fn query_err(e: rusqlite::Error) -> Error {
    Error::Query(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> rusqlite::Connection {
        rusqlite::Connection::open_in_memory().unwrap()
    }

    #[test]
    fn script_returns_last_result() {
        let c = mem();
        let r = run_script(
            &c,
            "create table t(a, b); insert into t values (1,'x'),(2,'y'); select * from t;",
            10,
        )
        .unwrap();
        assert_eq!(r.columns, ["a", "b"]);
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.rows[0], vec![Value::Int(1), Value::Text("x".into())]);
    }

    #[test]
    fn limit_truncates() {
        let c = mem();
        let r = run_script(&c, "with recursive n(i) as (select 1 union all select i+1 from n where i<50) select i from n", 5).unwrap();
        assert_eq!(r.rows.len(), 5);
        assert!(r.truncated);
    }

    #[test]
    fn dml_reports_rows_affected() {
        let c = mem();
        run_script(&c, "create table t(a)", 10).unwrap();
        let r = run_script(&c, "insert into t values (1),(2),(3)", 10).unwrap();
        assert_eq!(r.rows_affected, Some(3));
    }

    #[test]
    fn syntax_error_is_a_query_error() {
        assert!(matches!(run_script(&mem(), "selec 1", 10), Err(Error::Query(_))));
    }
}
