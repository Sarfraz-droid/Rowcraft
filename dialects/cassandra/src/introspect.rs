//! Reads `system_schema` into a [`Catalog`]: keyspaces are schemas; tables and views are tables.
//! The primary key is the partition key columns followed by the clustering columns.

use std::collections::HashMap;

use dg_dialect::{Connection, IntrospectScope, Result, ResultSet, Value};
use dg_model::{Catalog, Column, Constraint, ConstraintKind, Index, Schema, Table, TableKind};

const SYSTEM: &str = "'system', 'system_schema', 'system_auth', 'system_distributed', 'system_traces', 'system_views', 'system_virtual_schema'";

fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// CQL has no `IN` on non-key columns of `system_schema` tables, so scopes are applied in Rust.
fn in_scope(name: &str, scope: &IntrospectScope) -> bool {
    !SYSTEM.contains(&literal(name))
        && scope.schemas.as_ref().is_none_or(|s| s.iter().any(|n| n == name))
}

struct Raw {
    keyspaces: ResultSet,
    tables: ResultSet,
    views: ResultSet,
    columns: ResultSet,
    indexes: ResultSet,
}

pub(crate) async fn introspect(
    conn: &mut dyn Connection,
    scope: &IntrospectScope,
) -> Result<Catalog> {
    let raw = Raw {
        keyspaces: conn.query("select keyspace_name from system_schema.keyspaces").await?,
        tables: conn.query("select keyspace_name, table_name, comment from system_schema.tables").await?,
        // Views and indexes are optional features of older servers.
        views: conn
            .query("select keyspace_name, view_name, base_table_name from system_schema.views")
            .await
            .unwrap_or_default(),
        columns: conn
            .query("select keyspace_name, table_name, column_name, type, kind, position from system_schema.columns")
            .await?,
        indexes: conn
            .query("select keyspace_name, table_name, index_name, kind, options from system_schema.indexes")
            .await
            .unwrap_or_default(),
    };
    Ok(assemble(&raw, scope))
}

fn text(row: &[Value], i: usize) -> Option<String> {
    match row.get(i)? {
        Value::Null => None,
        Value::Text(s) => Some(s.clone()),
        Value::Int(n) => Some(n.to_string()),
        other => Some(format!("{other:?}")),
    }
}

fn int(row: &[Value], i: usize) -> i64 {
    match row.get(i) {
        Some(Value::Int(n)) => *n,
        Some(Value::Text(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

/// `target` of an index's options map, e.g. `{'target': 'name'}`.
fn index_target(options: &str) -> String {
    options
        .split_once("'target':")
        .map(|(_, rest)| rest.trim_start())
        .and_then(|rest| rest.strip_prefix('\''))
        .and_then(|rest| rest.split_once('\''))
        .map(|(t, _)| t.to_owned())
        .unwrap_or_default()
}

fn assemble(raw: &Raw, scope: &IntrospectScope) -> Catalog {
    let mut names: Vec<String> = raw
        .keyspaces
        .rows
        .iter()
        .filter_map(|r| text(r, 0))
        .filter(|n| in_scope(n, scope))
        .collect();
    names.sort();
    let mut schemas: Vec<Schema> =
        names.into_iter().map(|name| Schema { name, ..Schema::default() }).collect();
    let mut index: HashMap<(String, String), (usize, usize)> = HashMap::new();

    let mut add = |schemas: &mut Vec<Schema>,
                   ks: String,
                   name: String,
                   kind: TableKind,
                   comment: Option<String>| {
        let Some(si) = schemas.iter().position(|s| s.name == ks) else { return };
        index.insert((ks, name.clone()), (si, schemas[si].tables.len()));
        schemas[si].tables.push(Table {
            name,
            kind,
            comment: comment.filter(|c| !c.is_empty()),
            ..Table::default()
        });
    };
    let mut tables: Vec<_> = raw
        .tables
        .rows
        .iter()
        .filter_map(|r| Some((text(r, 0)?, text(r, 1)?, text(r, 2))))
        .collect();
    tables.sort();
    for (ks, name, comment) in tables {
        add(&mut schemas, ks, name, TableKind::Table, comment);
    }
    let mut views: Vec<_> =
        raw.views.rows.iter().filter_map(|r| Some((text(r, 0)?, text(r, 1)?))).collect();
    views.sort();
    for (ks, name) in views {
        add(&mut schemas, ks, name, TableKind::MaterializedView, None);
    }

    // Columns in key order: partition key, clustering, then regular/static by name.
    let mut cols: Vec<(String, String, i64, i64, String, String, String)> = raw
        .columns
        .rows
        .iter()
        .filter_map(|r| {
            let kind = text(r, 4)?;
            let rank = match kind.as_str() {
                "partition_key" => 0,
                "clustering" => 1,
                _ => 2,
            };
            Some((
                text(r, 0)?,
                text(r, 1)?,
                rank,
                int(r, 5),
                text(r, 2)?,
                text(r, 3).unwrap_or_default(),
                kind,
            ))
        })
        .collect();
    cols.sort();
    let mut keys: HashMap<(usize, usize), Vec<String>> = HashMap::new();
    for (ks, t, rank, _, name, ty, kind) in cols {
        let Some(&(si, ti)) = index.get(&(ks, t)) else { continue };
        if rank < 2 {
            keys.entry((si, ti)).or_default().push(name.clone());
        }
        schemas[si].tables[ti].columns.push(Column {
            name,
            native_type: if kind == "static" { format!("{ty} static") } else { ty },
            nullable: rank == 2,
            ..Column::default()
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
        let (Some(ks), Some(t), Some(name)) = (text(r, 0), text(r, 1), text(r, 2)) else {
            continue;
        };
        let Some(&(si, ti)) = index.get(&(ks, t)) else { continue };
        schemas[si].tables[ti].indexes.push(Index {
            name,
            unique: false,
            primary: false,
            method: text(r, 3).map(|k| k.to_ascii_lowercase()),
            columns: vec![index_target(&text(r, 4).unwrap_or_default())],
            predicate: None,
        });
    }
    Catalog { database: None, default_schema: None, schemas }
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
    fn partition_and_clustering_columns_form_the_key() {
        let raw = Raw {
            keyspaces: rs(vec![vec!["shop"], vec!["system"], vec!["other"]]),
            tables: rs(vec![vec!["shop", "events", "log"], vec!["system", "local", "~"]]),
            views: rs(vec![vec!["shop", "events_by_day", "events"]]),
            columns: rs(vec![
                vec!["shop", "events", "ts", "timestamp", "clustering", "0"],
                vec!["shop", "events", "id", "uuid", "partition_key", "0"],
                vec!["shop", "events", "note", "text", "regular", "-1"],
                vec!["shop", "events", "owner", "text", "static", "-1"],
            ]),
            indexes: rs(vec![vec![
                "shop",
                "events",
                "note_idx",
                "COMPOSITES",
                "{'target': 'note'}",
            ]]),
        };
        let cat = assemble(&raw, &IntrospectScope::default());
        assert_eq!(
            cat.schemas.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["other", "shop"]
        );
        let t = cat.table("shop", "events").unwrap();
        assert_eq!(t.primary_key(), ["id", "ts"]);
        assert_eq!(
            t.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["id", "ts", "note", "owner"]
        );
        assert!(!t.column("id").unwrap().nullable);
        assert!(t.column("note").unwrap().nullable);
        assert_eq!(t.column("owner").unwrap().native_type, "text static");
        assert_eq!(t.indexes[0].columns, ["note"]);
        assert_eq!(t.comment.as_deref(), Some("log"));
        assert_eq!(cat.table("shop", "events_by_day").unwrap().kind, TableKind::MaterializedView);
        assert!(cat.schema("system").is_none());
    }

    #[test]
    fn scope_limits_keyspaces() {
        let raw = Raw {
            keyspaces: rs(vec![vec!["a"], vec!["b"]]),
            tables: ResultSet::default(),
            views: ResultSet::default(),
            columns: ResultSet::default(),
            indexes: ResultSet::default(),
        };
        let cat = assemble(&raw, &IntrospectScope { schemas: Some(vec!["b".into()]) });
        assert_eq!(cat.schemas.len(), 1);
    }

    #[test]
    fn index_targets() {
        assert_eq!(index_target("{'target': 'name'}"), "name");
        assert_eq!(index_target("{}"), "");
    }
}
