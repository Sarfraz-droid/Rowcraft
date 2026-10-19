//! Runs against a real server. Skipped unless `ROWCRAFT_TEST_PG_PORT` is set:
//!
//! ```sh
//! ROWCRAFT_TEST_PG_PORT=5432 ROWCRAFT_TEST_PG_USER=postgres ROWCRAFT_TEST_PG_PASSWORD=... cargo test -p rowcraft-dialect-pg
//! ```
//! Optional: `ROWCRAFT_TEST_PG_HOST` (default `127.0.0.1`), `ROWCRAFT_TEST_PG_DB` (default `postgres`).

use rowcraft_config::{DataSourceConfig, SslMode};
use rowcraft_dialect::{Dialect, Error, Value};
use rowcraft_dialect_pg::PgDialect;

fn config() -> Option<DataSourceConfig> {
    let port = std::env::var("ROWCRAFT_TEST_PG_PORT").ok()?.parse().ok()?;
    Some(DataSourceConfig {
        name: "live".into(),
        dialect: "pg".into(),
        host: Some(std::env::var("ROWCRAFT_TEST_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into())),
        port: Some(port),
        database: Some(std::env::var("ROWCRAFT_TEST_PG_DB").unwrap_or_else(|_| "postgres".into())),
        user: std::env::var("ROWCRAFT_TEST_PG_USER").ok(),
        password: std::env::var("ROWCRAFT_TEST_PG_PASSWORD").ok(),
        ssl_mode: SslMode::Disable,
        ..Default::default()
    })
}

macro_rules! live_config {
    () => {
        match config() {
            Some(c) => c,
            None => {
                eprintln!("skipped: ROWCRAFT_TEST_PG_PORT not set");
                return;
            }
        }
    };
}

#[tokio::test]
async fn selects_values_and_null() {
    let cfg = live_config!();
    let mut conn = PgDialect.connect(&cfg).await.expect("connect");
    let rs = conn.query("select 1 + 1 as two, null::text as nothing").await.expect("query");
    assert_eq!(rs.columns, ["two", "nothing"]);
    assert_eq!(rs.rows, [vec![Value::Text("2".into()), Value::Null]]);
}

#[tokio::test]
async fn zero_row_result_keeps_its_columns() {
    let cfg = live_config!();
    let mut conn = PgDialect.connect(&cfg).await.expect("connect");
    let rs = conn.query("select 1 as a, 2 as b where false").await.expect("query");
    assert_eq!(rs.columns, ["a", "b"]);
    assert!(rs.rows.is_empty());
}

#[tokio::test]
async fn script_returns_the_last_statement() {
    let cfg = live_config!();
    let mut conn = PgDialect.connect(&cfg).await.expect("connect");
    let rs = conn.query("select 1 as a; select 2 as b").await.expect("query");
    assert_eq!(rs.columns, ["b"]);
}

#[tokio::test]
async fn sql_errors_are_query_errors_and_the_connection_survives() {
    let cfg = live_config!();
    let mut conn = PgDialect.connect(&cfg).await.expect("connect");
    let err = conn.query("select * from surely_missing_table").await.expect_err("must fail");
    assert!(matches!(err, Error::Query(ref m) if m.contains("surely_missing_table")), "{err}");
    assert!(conn.query("select 1").await.is_ok());
}

#[tokio::test]
async fn bad_password_is_a_connect_error() {
    let mut cfg = live_config!();
    cfg.password = Some("definitely-wrong".into());
    let err = PgDialect.connect(&cfg).await.expect_err("must fail");
    assert!(matches!(err, Error::Connect(_)), "{err}");
}

mod introspection {
    use rowcraft_dialect::IntrospectScope;
    use rowcraft_model::{ConstraintKind, FkAction, Identity, RoutineKind, TableKind};

    use super::*;

    /// Creates a throwaway schema, runs `body`, and always drops the schema again.
    const SETUP: &str = "
        drop schema if exists rowcraft_intro cascade;
        create schema rowcraft_intro;
        comment on schema rowcraft_intro is 'introspection fixture';
        create table rowcraft_intro.people (
            id integer generated always as identity primary key,
            name text not null,
            age integer default 18 check (age >= 0),
            doubled integer generated always as (age * 2) stored
        );
        comment on table rowcraft_intro.people is 'who';
        comment on column rowcraft_intro.people.name is 'full name';
        create index people_name_idx on rowcraft_intro.people (lower(name), age) where age > 0;
        create table rowcraft_intro.orders (
            id serial primary key,
            person_id integer not null references rowcraft_intro.people (id)
                on delete cascade on update restrict,
            unique (id, person_id)
        );
        create view rowcraft_intro.adults as select * from rowcraft_intro.people where age >= 18;
        create materialized view rowcraft_intro.adult_names as select name from rowcraft_intro.people;
        create function rowcraft_intro.add(a integer, b integer) returns integer
            language sql as 'select a + b';
        create procedure rowcraft_intro.noop() language sql as '';
    ";

    #[tokio::test]
    async fn reads_a_realistic_schema() {
        let cfg = live_config!();
        let mut conn = PgDialect.connect(&cfg).await.expect("connect");
        conn.query(SETUP).await.expect("setup");
        let scope = IntrospectScope { schemas: Some(vec!["rowcraft_intro".into()]) };
        let result = PgDialect.introspect(conn.as_mut(), &scope).await;
        conn.query("drop schema rowcraft_intro cascade").await.expect("cleanup");
        let catalog = result.expect("introspect");

        // Scope: only the requested schema, and never system schemas.
        assert_eq!(catalog.schemas.len(), 1);
        let schema = catalog.schema("rowcraft_intro").expect("schema");
        assert_eq!(schema.comment.as_deref(), Some("introspection fixture"));

        let kinds: Vec<_> = schema.tables.iter().map(|t| (t.name.as_str(), t.kind)).collect();
        assert_eq!(
            kinds,
            [
                ("adult_names", TableKind::MaterializedView),
                ("adults", TableKind::View),
                ("orders", TableKind::Table),
                ("people", TableKind::Table),
            ]
        );

        let people = schema.table("people").unwrap();
        assert_eq!(people.comment.as_deref(), Some("who"));
        assert!(people.state.is_some());
        let id = people.column("id").unwrap();
        assert_eq!(id.identity, Some(Identity::Always));
        assert!(!id.nullable);
        let name = people.column("name").unwrap();
        assert_eq!((name.native_type.as_str(), name.nullable), ("text", false));
        assert_eq!(name.comment.as_deref(), Some("full name"));
        assert_eq!(people.column("age").unwrap().default.as_deref(), Some("18"));
        let doubled = people.column("doubled").unwrap();
        assert!(doubled.generated);
        assert_eq!(doubled.default.as_deref(), Some("(age * 2)"));
        assert_eq!(people.primary_key(), ["id"]);

        let idx = people.indexes.iter().find(|i| i.name == "people_name_idx").unwrap();
        assert_eq!(idx.columns, ["lower(name)", "age"]);
        assert_eq!(idx.predicate.as_deref(), Some("(age > 0)"));
        assert_eq!(idx.method.as_deref(), Some("btree"));
        assert!(!idx.unique);
        assert!(people.indexes.iter().any(|i| i.primary));
        assert!(people.constraints.iter().any(|c| c.kind == ConstraintKind::Check));

        let orders = schema.table("orders").unwrap();
        let fk = orders.foreign_keys().next().expect("fk");
        assert_eq!(fk.columns, ["person_id"]);
        let target = fk.reference.as_ref().unwrap();
        assert_eq!((target.schema.as_str(), target.table.as_str()), ("rowcraft_intro", "people"));
        assert_eq!(target.columns, ["id"]);
        assert_eq!((target.on_update, target.on_delete), (FkAction::Restrict, FkAction::Cascade));
        let unique = orders.constraints.iter().find(|c| c.kind == ConstraintKind::Unique).unwrap();
        assert_eq!(unique.columns, ["id", "person_id"]);

        // `serial` owns a visible sequence; the identity column's hidden one is not listed.
        let seqs: Vec<_> = schema.sequences.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(seqs, ["orders_id_seq"]);

        let add = schema.routines.iter().find(|r| r.name == "add").unwrap();
        assert_eq!(add.kind, RoutineKind::Function);
        assert_eq!(add.arguments, "a integer, b integer");
        assert_eq!(add.returns.as_deref(), Some("integer"));
        let noop = schema.routines.iter().find(|r| r.name == "noop").unwrap();
        assert_eq!((noop.kind, noop.returns.as_deref()), (RoutineKind::Procedure, None));
    }

    #[tokio::test]
    async fn skips_system_schemas_and_leaves_the_connection_usable() {
        let cfg = live_config!();
        let mut conn = PgDialect.connect(&cfg).await.expect("connect");
        let catalog =
            PgDialect.introspect(conn.as_mut(), &IntrospectScope::default()).await.expect("ok");
        assert!(catalog.schema("pg_catalog").is_none());
        assert!(catalog.schema("information_schema").is_none());
        // The introspection transaction must be closed: a plain statement still works.
        assert!(conn.query("select 1").await.is_ok());
    }

    #[tokio::test]
    async fn an_empty_scope_returns_nothing() {
        let cfg = live_config!();
        let mut conn = PgDialect.connect(&cfg).await.expect("connect");
        let scope = IntrospectScope { schemas: Some(vec![]) };
        let catalog = PgDialect.introspect(conn.as_mut(), &scope).await.expect("ok");
        assert!(catalog.schemas.is_empty());
    }

    #[tokio::test]
    async fn schema_names_with_quotes_do_not_break_the_query() {
        let cfg = live_config!();
        let mut conn = PgDialect.connect(&cfg).await.expect("connect");
        let scope = IntrospectScope { schemas: Some(vec!["it's \\ odd".into()]) };
        let catalog = PgDialect.introspect(conn.as_mut(), &scope).await.expect("ok");
        assert!(catalog.schemas.is_empty());
    }
}
