//! Reads ClickHouse's `system` tables into a [`Catalog`]. A database is a schema.
//!
//! ClickHouse has no foreign keys. Its "primary key" only orders data and does not enforce
//! uniqueness; it is reported as key metadata without implying enforced uniqueness.

use std::collections::HashMap;

use rowcraft_dialect::{Connection, IntrospectScope, Result, ResultSet, Value};
use rowcraft_model::{
    Catalog, Column, Constraint, ConstraintKind, Index, Routine, RoutineKind, Schema, Table,
    TableKind,
};

const SYSTEM: &str = "'system', 'INFORMATION_SCHEMA', 'information_schema'";

fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn filter(col: &str, scope: &IntrospectScope) -> String {
    let mut out = format!("{col} not in ({SYSTEM})");
    if let Some(only) = &scope.schemas {
        let list = only.iter().map(|s| literal(s)).collect::<Vec<_>>().join(", ");
        out.push_str(&format!(
            " and {col} in ({})",
            if list.is_empty() { "''".into() } else { list }
        ));
    }
    out
}

struct Raw {
    session: ResultSet,
    schemas: ResultSet,
    tables: ResultSet,
    columns: ResultSet,
    indexes: ResultSet,
    functions: ResultSet,
}

pub(crate) async fn introspect(
    conn: &mut dyn Connection,
    scope: &IntrospectScope,
) -> Result<Catalog> {
    let raw = Raw {
        session: conn.query("select currentDatabase()").await?,
        schemas: conn
            .query(&format!(
                "select name, comment from system.databases where {} order by name",
                filter("name", scope)
            ))
            .await?,
        tables: conn
            .query(&format!(
                "select database, name, engine, comment, toString(metadata_modification_time) \
                 from system.tables where is_temporary = 0 and {} order by database, name",
                filter("database", scope)
            ))
            .await?,
        columns: conn
            .query(&format!(
                "select database, table, name, type, default_kind, default_expression, comment, \
                 is_in_primary_key from system.columns where {} order by database, table, position",
                filter("database", scope)
            ))
            .await?,
        // Absent before 20.x; an empty result is the right answer there.
        indexes: conn
            .query(&format!(
                "select database, table, name, type, expr from system.data_skipping_indices where {} \
                 order by database, table, name",
                filter("database", scope)
            ))
            .await
            .unwrap_or_default(),
        functions: conn
            .query("select name, create_query from system.functions where origin = 'SQLUserDefined' order by name")
            .await
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
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
    }
}

fn kind_of(engine: &str) -> TableKind {
    match engine {
        "View" | "LiveView" | "WindowView" => TableKind::View,
        "MaterializedView" => TableKind::MaterializedView,
        _ => TableKind::Table,
    }
}

fn assemble(raw: &Raw) -> Catalog {
    let mut schemas: Vec<Schema> = raw
        .schemas
        .rows
        .iter()
        .filter_map(|r| {
            Some(Schema {
                name: text(r, 0)?,
                comment: text(r, 1).filter(|c| !c.is_empty()),
                ..Schema::default()
            })
        })
        .collect();
    let mut index: HashMap<(String, String), (usize, usize)> = HashMap::new();
    for r in &raw.tables.rows {
        let (Some(d), Some(t)) = (text(r, 0), text(r, 1)) else { continue };
        let si = match schemas.iter().position(|s| s.name == d) {
            Some(i) => i,
            None => {
                schemas.push(Schema { name: d.clone(), ..Schema::default() });
                schemas.len() - 1
            }
        };
        index.insert((d, t.clone()), (si, schemas[si].tables.len()));
        schemas[si].tables.push(Table {
            name: t,
            kind: kind_of(&text(r, 2).unwrap_or_default()),
            comment: text(r, 3).filter(|c| !c.is_empty()),
            state: text(r, 4),
            ..Table::default()
        });
    }

    let mut keys: HashMap<(usize, usize), Vec<String>> = HashMap::new();
    for r in &raw.columns.rows {
        let (Some(d), Some(t), Some(name)) = (text(r, 0), text(r, 1), text(r, 2)) else { continue };
        let Some(&(si, ti)) = index.get(&(d, t)) else { continue };
        let native_type = text(r, 3).unwrap_or_default();
        let kind = text(r, 4).unwrap_or_default();
        if text(r, 7).as_deref() == Some("1") {
            keys.entry((si, ti)).or_default().push(name.clone());
        }
        schemas[si].tables[ti].columns.push(Column {
            name,
            nullable: native_type.starts_with("Nullable("),
            native_type,
            // MATERIALIZED / ALIAS columns are computed; DEFAULT and EPHEMERAL are plain defaults.
            generated: matches!(kind.as_str(), "MATERIALIZED" | "ALIAS"),
            default: text(r, 5).filter(|e| !e.is_empty()),
            identity: None,
            comment: text(r, 6).filter(|c| !c.is_empty()),
        });
    }
    for ((si, ti), columns) in keys {
        schemas[si].tables[ti].constraints.push(Constraint {
            name: "PRIMARY".into(),
            kind: ConstraintKind::PrimaryKey,
            definition: format!("PRIMARY KEY ({})", columns.join(", ")),
            columns,
            reference: None,
        });
    }

    for r in &raw.indexes.rows {
        let (Some(d), Some(t), Some(name)) = (text(r, 0), text(r, 1), text(r, 2)) else { continue };
        let Some(&(si, ti)) = index.get(&(d, t)) else { continue };
        schemas[si].tables[ti].indexes.push(Index {
            name,
            unique: false,
            primary: false,
            method: text(r, 3),
            columns: vec![text(r, 4).unwrap_or_default()],
            predicate: None,
        });
    }

    // User-defined functions are global; show them under the current database.
    let current = raw.session.rows.first().and_then(|r| text(r, 0));
    if let Some(si) = current.as_ref().and_then(|c| schemas.iter().position(|s| &s.name == c)) {
        for r in &raw.functions.rows {
            let Some(name) = text(r, 0) else { continue };
            schemas[si].routines.push(Routine {
                name,
                kind: RoutineKind::Function,
                arguments: text(r, 1)
                    .and_then(|q| Some(q.split_once('(')?.1.split_once(')')?.0.to_owned()))
                    .unwrap_or_default(),
                returns: None,
            });
        }
    }

    Catalog { database: current.clone(), default_schema: current, schemas }
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
    fn assembles_tables_keys_and_indexes() {
        let raw = Raw {
            session: rs(vec![vec!["app"]]),
            schemas: rs(vec![vec!["app", ""]]),
            tables: rs(vec![
                vec!["app", "events", "MergeTree", "log", "2026-01-01"],
                vec!["app", "v", "MaterializedView", "", "~"],
            ]),
            columns: rs(vec![
                vec!["app", "events", "id", "UInt64", "", "", "", "1"],
                vec!["app", "events", "note", "Nullable(String)", "", "", "free text", "0"],
                vec!["app", "events", "day", "Date", "MATERIALIZED", "toDate(ts)", "", "0"],
            ]),
            indexes: rs(vec![vec!["app", "events", "idx_note", "bloom_filter", "note"]]),
            functions: rs(vec![vec!["plus_one", "CREATE FUNCTION plus_one AS (x) -> (x + 1)"]]),
        };
        let cat = assemble(&raw);
        let t = cat.table("app", "events").unwrap();
        assert_eq!(t.primary_key(), ["id"]);
        assert!(t.column("note").unwrap().nullable);
        assert!(!t.column("id").unwrap().nullable);
        assert!(t.column("day").unwrap().generated);
        assert_eq!(t.indexes[0].method.as_deref(), Some("bloom_filter"));
        assert_eq!(cat.table("app", "v").unwrap().kind, TableKind::MaterializedView);
        assert_eq!(cat.schema("app").unwrap().routines[0].name, "plus_one");
    }

    #[test]
    fn filter_escapes_with_backslash() {
        assert!(
            filter("d", &IntrospectScope { schemas: Some(vec!["a'b".into()]) })
                .contains("d in ('a\\'b')")
        );
    }
}
