//! Shared Postgres wire/catalog helpers (pg, Redshift, Greenplum, CockroachDB, Vertica).
//!
//! `rowcraft-dialect-pg` reads `pg_catalog` directly and needs PostgreSQL 12. The databases that only
//! look like PostgreSQL (Redshift is 8.0-based, CockroachDB has a partial `pg_catalog`, old
//! Greenplum is 9.4-based) all have a working `information_schema`, so this crate introspects
//! through that: schemas, tables and views, columns, primary keys, unique and foreign keys.

use std::collections::HashMap;

use rowcraft_dialect::{Connection, IntrospectScope, Result, ResultSet, Value};
use rowcraft_model::{
    Catalog, Column, Constraint, ConstraintKind, FkAction, ForeignKeyTarget, Identity, Schema,
    Table, TableKind,
};

/// Schemas that belong to the database itself.
const SYSTEM_SCHEMAS: &[&str] = &[
    "information_schema",
    "pg_catalog",
    "pg_toast",
    "pg_internal",
    "pg_automv",
    "crdb_internal",
    "gp_toolkit",
    "pg_aoseg",
    "pg_bitmapindex",
];

fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// `and <col> not in (...) [and <col> in (...)]`, shared by every query.
fn schema_filter(col: &str, scope: &IntrospectScope) -> String {
    let system = SYSTEM_SCHEMAS.iter().map(|s| literal(s)).collect::<Vec<_>>().join(", ");
    let mut out = format!("{col} not in ({system}) and {col} not like 'pg\\_temp\\_%'");
    if let Some(only) = &scope.schemas {
        let list = only.iter().map(|s| literal(s)).collect::<Vec<_>>().join(", ");
        out.push_str(&format!(
            " and {col} in ({})",
            if list.is_empty() { "null".into() } else { list }
        ));
    }
    out
}

struct Raw {
    session: ResultSet,
    schemas: ResultSet,
    tables: ResultSet,
    columns: ResultSet,
    keys: ResultSet,
    references: ResultSet,
}

/// Reads the catalog through `information_schema`.
pub async fn introspect_information_schema(
    conn: &mut dyn Connection,
    scope: &IntrospectScope,
) -> Result<Catalog> {
    let f = |c: &str| schema_filter(c, scope);
    let raw = Raw {
        session: conn.query("select current_database(), current_schema()").await?,
        schemas: conn
            .query(&format!(
                "select schema_name from information_schema.schemata where {} order by 1",
                f("schema_name")
            ))
            .await?,
        tables: conn
            .query(&format!(
                "select table_schema, table_name, table_type from information_schema.tables \
                 where {} order by 1, 2",
                f("table_schema")
            ))
            .await?,
        columns: conn
            .query(&format!(
                "select table_schema, table_name, column_name, data_type, character_maximum_length, \
                 numeric_precision, numeric_scale, is_nullable, column_default, is_identity, \
                 identity_generation \
                 from information_schema.columns where {} order by table_schema, table_name, ordinal_position",
                f("table_schema")
            ))
            .await?,
        keys: conn
            .query(&format!(
                "select tc.table_schema, tc.table_name, tc.constraint_name, tc.constraint_type, \
                 kcu.column_name, kcu.ordinal_position \
                 from information_schema.table_constraints tc \
                 join information_schema.key_column_usage kcu \
                   on kcu.constraint_schema = tc.constraint_schema \
                  and kcu.constraint_name = tc.constraint_name \
                  and kcu.table_name = tc.table_name \
                 where tc.constraint_type in ('PRIMARY KEY', 'UNIQUE') and {} \
                 order by 1, 2, 3, kcu.ordinal_position",
                f("tc.table_schema")
            ))
            .await
            // Keys are optional: not every PostgreSQL-protocol database exposes these views.
            .unwrap_or_default(),
        references: conn
            .query(&format!(
                "select kcu.table_schema, kcu.table_name, kcu.constraint_name, kcu.column_name, \
                 kcu.ordinal_position, ccu.table_schema, ccu.table_name, ccu.column_name, \
                 rc.update_rule, rc.delete_rule \
                 from information_schema.referential_constraints rc \
                 join information_schema.key_column_usage kcu \
                   on kcu.constraint_schema = rc.constraint_schema \
                  and kcu.constraint_name = rc.constraint_name \
                 join information_schema.constraint_column_usage ccu \
                   on ccu.constraint_schema = rc.unique_constraint_schema \
                  and ccu.constraint_name = rc.unique_constraint_name \
                 where {} order by 1, 2, 3, kcu.ordinal_position",
                f("kcu.table_schema")
            ))
            .await
            // Redshift and some Greenplum builds do not expose every view; keys are optional.
            .unwrap_or_default(),
    };
    Ok(assemble(&raw))
}

fn text(row: &[Value], i: usize) -> Option<String> {
    match row.get(i)? {
        Value::Null => None,
        Value::Text(s) => Some(s.clone()),
        Value::Int(n) => Some(n.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Bytes(_) => None,
    }
}

fn action(s: Option<String>) -> FkAction {
    match s.as_deref() {
        Some("CASCADE") => FkAction::Cascade,
        Some("RESTRICT") => FkAction::Restrict,
        Some("SET NULL") => FkAction::SetNull,
        Some("SET DEFAULT") => FkAction::SetDefault,
        _ => FkAction::NoAction,
    }
}

/// `character varying` plus length/precision, the way the catalog would print it.
fn native_type(r: &[Value]) -> String {
    let base = text(r, 3).unwrap_or_default();
    let len = text(r, 4);
    let precision = text(r, 5);
    let scale = text(r, 6);
    match (base.as_str(), len, precision, scale) {
        ("character varying" | "character", Some(l), _, _) => format!("{base}({l})"),
        ("numeric", _, Some(p), Some(s)) => format!("numeric({p},{s})"),
        _ => base,
    }
}

fn assemble(raw: &Raw) -> Catalog {
    let mut schemas: Vec<Schema> = raw
        .schemas
        .rows
        .iter()
        .filter_map(|r| text(r, 0))
        .map(|name| Schema { name, ..Schema::default() })
        .collect();
    let mut index: HashMap<(String, String), (usize, usize)> = HashMap::new();
    for r in &raw.tables.rows {
        let (Some(s), Some(t)) = (text(r, 0), text(r, 1)) else { continue };
        let si = match schemas.iter().position(|x| x.name == s) {
            Some(i) => i,
            None => {
                schemas.push(Schema { name: s.clone(), ..Schema::default() });
                schemas.len() - 1
            }
        };
        let kind = match text(r, 2).as_deref() {
            Some("VIEW") => TableKind::View,
            Some("FOREIGN") | Some("FOREIGN TABLE") => TableKind::ForeignTable,
            _ => TableKind::Table,
        };
        index.insert((s, t.clone()), (si, schemas[si].tables.len()));
        schemas[si].tables.push(Table { name: t, kind, ..Table::default() });
    }

    for r in &raw.columns.rows {
        let (Some(s), Some(t), Some(name)) = (text(r, 0), text(r, 1), text(r, 2)) else { continue };
        let Some(&(si, ti)) = index.get(&(s, t)) else { continue };
        let identity = (text(r, 9).as_deref() == Some("YES")).then(|| {
            if text(r, 10).as_deref() == Some("ALWAYS") {
                Identity::Always
            } else {
                Identity::ByDefault
            }
        });
        schemas[si].tables[ti].columns.push(Column {
            name,
            native_type: native_type(r),
            nullable: text(r, 7).as_deref() != Some("NO"),
            default: text(r, 8),
            identity,
            generated: false,
            comment: None,
        });
    }

    // Primary and unique keys: consecutive rows with the same constraint form one key.
    let mut last: Option<(String, String, String)> = None;
    for r in &raw.keys.rows {
        let (Some(s), Some(t), Some(c), Some(kind), Some(col)) =
            (text(r, 0), text(r, 1), text(r, 2), text(r, 3), text(r, 4))
        else {
            continue;
        };
        let Some(&(si, ti)) = index.get(&(s.clone(), t.clone())) else { continue };
        let table = &mut schemas[si].tables[ti];
        let key = (s, t, c.clone());
        if last.as_ref() == Some(&key) {
            if let Some(con) = table.constraints.last_mut() {
                con.columns.push(col);
            }
        } else {
            let kind = if kind == "PRIMARY KEY" {
                ConstraintKind::PrimaryKey
            } else {
                ConstraintKind::Unique
            };
            table.constraints.push(Constraint {
                name: c,
                kind,
                columns: vec![col],
                definition: String::new(),
                reference: None,
            });
            last = Some(key);
        }
    }
    for schema in &mut schemas {
        for table in &mut schema.tables {
            for con in &mut table.constraints {
                if con.reference.is_none() && con.definition.is_empty() {
                    let label = if con.kind == ConstraintKind::PrimaryKey {
                        "PRIMARY KEY"
                    } else {
                        "UNIQUE"
                    };
                    con.definition = format!("{label} ({})", con.columns.join(", "));
                }
            }
        }
    }

    let mut last: Option<(String, String, String)> = None;
    for r in &raw.references.rows {
        let (Some(s), Some(t), Some(c), Some(col)) =
            (text(r, 0), text(r, 1), text(r, 2), text(r, 3))
        else {
            continue;
        };
        let (Some(ps), Some(pt), Some(pcol)) = (text(r, 5), text(r, 6), text(r, 7)) else {
            continue;
        };
        let Some(&(si, ti)) = index.get(&(s.clone(), t.clone())) else { continue };
        let table = &mut schemas[si].tables[ti];
        let key = (s, t, c.clone());
        if last.as_ref() == Some(&key) {
            if let Some(con) = table.constraints.last_mut() {
                con.columns.push(col);
                if let Some(target) = con.reference.as_mut() {
                    target.columns.push(pcol);
                }
            }
        } else {
            table.constraints.push(Constraint {
                name: c,
                kind: ConstraintKind::ForeignKey,
                columns: vec![col],
                definition: String::new(),
                reference: Some(ForeignKeyTarget {
                    schema: ps,
                    table: pt,
                    columns: vec![pcol],
                    on_update: action(text(r, 8)),
                    on_delete: action(text(r, 9)),
                }),
            });
            last = Some(key);
        }
    }
    for schema in &mut schemas {
        for table in &mut schema.tables {
            for con in &mut table.constraints {
                if let (ConstraintKind::ForeignKey, Some(t)) = (con.kind, &con.reference) {
                    con.definition = format!(
                        "FOREIGN KEY ({}) REFERENCES {}.{} ({})",
                        con.columns.join(", "),
                        t.schema,
                        t.table,
                        t.columns.join(", ")
                    );
                }
            }
        }
    }

    let session = raw.session.rows.first();
    Catalog {
        database: session.and_then(|r| text(r, 0)),
        default_schema: session.and_then(|r| text(r, 1)),
        schemas,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rs(rows: Vec<Vec<&str>>) -> ResultSet {
        ResultSet {
            rows: rows
                .into_iter()
                .map(|r| {
                    r.into_iter()
                        .map(|s| if s == "~" { Value::Null } else { Value::Text(s.into()) })
                        .collect()
                })
                .collect(),
            ..ResultSet::default()
        }
    }

    #[test]
    fn assembles_tables_keys_and_references() {
        let raw = Raw {
            session: rs(vec![vec!["app", "public"]]),
            schemas: rs(vec![vec!["public"]]),
            tables: rs(vec![
                vec!["public", "a", "BASE TABLE"],
                vec!["public", "b", "BASE TABLE"],
                vec!["public", "v", "VIEW"],
            ]),
            columns: rs(vec![
                vec!["public", "a", "id", "integer", "~", "32", "0", "NO", "~", "NO", "~"],
                vec![
                    "public",
                    "a",
                    "name",
                    "character varying",
                    "40",
                    "~",
                    "~",
                    "YES",
                    "~",
                    "NO",
                    "~",
                ],
                vec!["public", "b", "a_id", "integer", "~", "32", "0", "NO", "~", "YES", "ALWAYS"],
            ]),
            keys: rs(vec![
                vec!["public", "a", "a_pkey", "PRIMARY KEY", "id", "1"],
                vec!["public", "b", "b_u", "UNIQUE", "a_id", "1"],
            ]),
            references: rs(vec![vec![
                "public",
                "b",
                "b_fk",
                "a_id",
                "1",
                "public",
                "a",
                "id",
                "CASCADE",
                "NO ACTION",
            ]]),
        };
        let cat = assemble(&raw);
        assert_eq!(cat.database.as_deref(), Some("app"));
        let a = cat.table("public", "a").unwrap();
        assert_eq!(a.primary_key(), ["id"]);
        assert_eq!(a.column("name").unwrap().native_type, "character varying(40)");
        assert!(a.column("name").unwrap().nullable);
        let b = cat.table("public", "b").unwrap();
        assert_eq!(b.column("a_id").unwrap().identity, Some(Identity::Always));
        let fk = b.foreign_keys().next().unwrap();
        assert_eq!(fk.reference.as_ref().unwrap().on_update, FkAction::Cascade);
        assert_eq!(fk.definition, "FOREIGN KEY (a_id) REFERENCES public.a (id)");
        assert_eq!(cat.table("public", "v").unwrap().kind, TableKind::View);
    }

    #[test]
    fn filter_skips_system_schemas_and_honours_scope() {
        let f = schema_filter("s", &IntrospectScope { schemas: Some(vec!["it's".into()]) });
        assert!(f.contains("'pg_catalog'"));
        assert!(f.contains("s in ('it''s')"));
    }
}
