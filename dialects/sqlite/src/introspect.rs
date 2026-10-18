//! Reads `sqlite_master` and the `pragma_*` table-valued functions into a [`Catalog`].
//!
//! SQLite has one schema per attached database; only `main` is read. Each object kind is one
//! query joined against `sqlite_master`, and [`assemble`] is pure so it is tested without a file.

use std::collections::HashMap;

use dg_dialect::{Connection, IntrospectScope, Result, ResultSet, Value};
use dg_model::{
    Catalog, Column, Constraint, ConstraintKind, FkAction, ForeignKeyTarget, Identity, Index,
    Schema, Table, TableKind,
};

const SCHEMA: &str = "main";

const TABLES: &str = "select name, type, sql from sqlite_master \
    where type in ('table','view') and name not like 'sqlite\\_%' escape '\\' order by name";

const COLUMNS: &str = "select m.name, c.name, c.type, c.\"notnull\", c.dflt_value, c.pk, c.hidden \
    from sqlite_master m, pragma_table_xinfo(m.name) c \
    where m.type in ('table','view') and m.name not like 'sqlite\\_%' escape '\\' \
    order by m.name, c.cid";

const INDEXES: &str = "select m.name, il.name, il.\"unique\", il.origin, il.partial, \
    (select sql from sqlite_master where name = il.name and type = 'index') \
    from sqlite_master m, pragma_index_list(m.name) il \
    where m.type = 'table' and m.name not like 'sqlite\\_%' escape '\\' order by m.name, il.seq";

const INDEX_COLUMNS: &str = "select il.name, ii.name \
    from sqlite_master m, pragma_index_list(m.name) il, pragma_index_xinfo(il.name) ii \
    where m.type = 'table' and m.name not like 'sqlite\\_%' escape '\\' and ii.key = 1 \
    order by il.name, ii.seqno";

const FOREIGN_KEYS: &str = "select m.name, f.id, f.\"table\", f.\"from\", f.\"to\", f.on_update, f.on_delete \
    from sqlite_master m, pragma_foreign_key_list(m.name) f \
    where m.type = 'table' order by m.name, f.id, f.seq";

pub(crate) async fn introspect(
    conn: &mut dyn Connection,
    scope: &IntrospectScope,
) -> Result<Catalog> {
    if scope.schemas.as_ref().is_some_and(|s| !s.iter().any(|n| n == SCHEMA)) {
        return Ok(Catalog {
            database: None,
            default_schema: Some(SCHEMA.into()),
            schemas: vec![],
        });
    }
    let raw = Raw {
        tables: conn.query(TABLES).await?,
        columns: conn.query(COLUMNS).await?,
        indexes: conn.query(INDEXES).await?,
        index_columns: conn.query(INDEX_COLUMNS).await?,
        foreign_keys: conn.query(FOREIGN_KEYS).await?,
    };
    Ok(assemble(&raw))
}

struct Raw {
    tables: ResultSet,
    columns: ResultSet,
    indexes: ResultSet,
    index_columns: ResultSet,
    foreign_keys: ResultSet,
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

fn int(row: &[Value], i: usize) -> i64 {
    match row.get(i) {
        Some(Value::Int(n)) => *n,
        Some(Value::Text(s)) => s.parse().unwrap_or(0),
        Some(Value::Bool(b)) => i64::from(*b),
        _ => 0,
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

/// Text after the last top-level `WHERE` of a `CREATE INDEX`, for partial indexes.
fn predicate(sql: &str) -> Option<String> {
    let upper = sql.to_ascii_uppercase();
    let at = upper.rfind(" WHERE ")?;
    Some(sql[at + 7..].trim().to_owned())
}

fn assemble(raw: &Raw) -> Catalog {
    let mut tables: Vec<Table> = Vec::new();
    let mut position: HashMap<String, usize> = HashMap::new();
    for r in &raw.tables.rows {
        let Some(name) = text(r, 0) else { continue };
        let kind =
            if text(r, 1).as_deref() == Some("view") { TableKind::View } else { TableKind::Table };
        position.insert(name.clone(), tables.len());
        tables.push(Table { name, kind, ..Table::default() });
    }

    // Primary key columns in key order, per table.
    let mut pk: HashMap<usize, Vec<(i64, String)>> = HashMap::new();
    for r in &raw.columns.rows {
        let (Some(t), Some(name)) = (text(r, 0), text(r, 1)) else { continue };
        let Some(&ti) = position.get(&t) else { continue };
        let hidden = int(r, 6);
        // 1 = hidden virtual-table column; 2 and 3 are generated columns.
        if hidden == 1 {
            continue;
        }
        let pk_ordinal = int(r, 5);
        if pk_ordinal > 0 {
            pk.entry(ti).or_default().push((pk_ordinal, name.clone()));
        }
        let native_type = text(r, 2).unwrap_or_default();
        let table_sql = raw
            .tables
            .rows
            .iter()
            .find(|x| text(x, 0).as_deref() == Some(&t))
            .and_then(|x| text(x, 2));
        // A lone INTEGER PRIMARY KEY is an alias for the rowid and fills itself in.
        let rowid_alias = pk_ordinal == 1 && native_type.eq_ignore_ascii_case("INTEGER");
        let auto = rowid_alias
            && table_sql.is_some_and(|s| s.to_ascii_uppercase().contains("AUTOINCREMENT"));
        tables[ti].columns.push(Column {
            name,
            native_type,
            nullable: int(r, 3) == 0 && pk_ordinal == 0,
            default: text(r, 4),
            identity: auto.then_some(Identity::ByDefault),
            generated: hidden >= 2,
            comment: None,
        });
    }
    for (ti, mut cols) in pk {
        cols.sort();
        // A table whose PK is a single rowid alias still has a constraint worth showing.
        let name = format!("{}_pk", tables[ti].name);
        tables[ti].constraints.push(Constraint {
            name,
            kind: ConstraintKind::PrimaryKey,
            definition: format!(
                "PRIMARY KEY ({})",
                cols.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>().join(", ")
            ),
            columns: cols.into_iter().map(|(_, c)| c).collect(),
            reference: None,
        });
    }

    let mut index_cols: HashMap<String, Vec<String>> = HashMap::new();
    for r in &raw.index_columns.rows {
        if let (Some(i), Some(c)) = (text(r, 0), text(r, 1)) {
            index_cols.entry(i).or_default().push(c);
        }
    }
    for r in &raw.indexes.rows {
        let (Some(t), Some(name)) = (text(r, 0), text(r, 1)) else { continue };
        let Some(&ti) = position.get(&t) else { continue };
        let origin = text(r, 3).unwrap_or_default();
        let columns = index_cols.get(&name).cloned().unwrap_or_default();
        let unique = int(r, 2) != 0;
        let primary = origin == "pk";
        // The implicit index behind a PRIMARY KEY duplicates the PK constraint.
        if !primary {
            tables[ti].indexes.push(Index {
                name: name.clone(),
                unique,
                primary: false,
                method: Some("btree".into()),
                columns: columns.clone(),
                predicate: if int(r, 4) != 0 {
                    text(r, 5).and_then(|s| predicate(&s))
                } else {
                    None
                },
            });
        }
        if origin == "u" {
            tables[ti].constraints.push(Constraint {
                name,
                kind: ConstraintKind::Unique,
                definition: format!("UNIQUE ({})", columns.join(", ")),
                columns,
                reference: None,
            });
        }
    }

    // Foreign keys: rows of one `id` form one (possibly composite) key.
    let mut groups: Vec<(String, i64, Vec<&Vec<Value>>)> = Vec::new();
    for r in &raw.foreign_keys.rows {
        let (Some(t), id) = (text(r, 0), int(r, 1)) else { continue };
        match groups.last_mut() {
            Some((gt, gid, rows)) if *gt == t && *gid == id => rows.push(r),
            _ => groups.push((t, id, vec![r])),
        }
    }
    for (t, id, rows) in groups {
        let Some(&ti) = position.get(&t) else { continue };
        let target = text(rows[0], 2).unwrap_or_default();
        let columns: Vec<String> = rows.iter().filter_map(|r| text(r, 3)).collect();
        // `to` is NULL when the key points at the parent's primary key.
        let mut ref_cols: Vec<String> = rows.iter().filter_map(|r| text(r, 4)).collect();
        if ref_cols.is_empty() {
            ref_cols = position
                .get(&target)
                .map(|&i| tables[i].primary_key().to_vec())
                .unwrap_or_default();
        }
        let definition = format!(
            "FOREIGN KEY ({}) REFERENCES {target} ({})",
            columns.join(", "),
            ref_cols.join(", ")
        );
        tables[ti].constraints.push(Constraint {
            name: format!("{t}_fk{id}"),
            kind: ConstraintKind::ForeignKey,
            columns,
            definition,
            reference: Some(ForeignKeyTarget {
                schema: SCHEMA.into(),
                table: target,
                columns: ref_cols,
                on_update: action(text(rows[0], 5)),
                on_delete: action(text(rows[0], 6)),
            }),
        });
    }

    Catalog {
        database: None,
        default_schema: Some(SCHEMA.into()),
        schemas: vec![Schema { name: SCHEMA.into(), tables, ..Schema::default() }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_script;

    fn fetch(sql: &str) -> Catalog {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch(sql).unwrap();
        let q = |s: &str| run_script(&c, s, usize::MAX).unwrap();
        assemble(&Raw {
            tables: q(TABLES),
            columns: q(COLUMNS),
            indexes: q(INDEXES),
            index_columns: q(INDEX_COLUMNS),
            foreign_keys: q(FOREIGN_KEYS),
        })
    }

    #[test]
    fn reads_tables_columns_keys_and_indexes() {
        let cat = fetch(
            "create table parent(id integer primary key autoincrement, code text not null unique);
             create table child(id integer, parent_id integer references parent(id) on delete cascade,
               n int default 5, primary key (id, parent_id));
             create index child_n on child(n) where n > 1;
             create view v as select * from child;",
        );
        let main = cat.schema("main").unwrap();
        assert_eq!(main.tables.len(), 3);
        let parent = cat.table("main", "parent").unwrap();
        assert_eq!(parent.primary_key(), ["id"]);
        assert_eq!(parent.column("id").unwrap().identity, Some(Identity::ByDefault));
        assert!(!parent.column("code").unwrap().nullable);
        assert!(parent.constraints.iter().any(|c| c.kind == ConstraintKind::Unique));
        let child = cat.table("main", "child").unwrap();
        assert_eq!(child.primary_key(), ["id", "parent_id"]);
        assert_eq!(child.column("n").unwrap().default.as_deref(), Some("5"));
        let fk = child.foreign_keys().next().unwrap();
        assert_eq!(fk.reference.as_ref().unwrap().on_delete, FkAction::Cascade);
        assert_eq!(fk.reference.as_ref().unwrap().columns, ["id"]);
        let idx = child.indexes.iter().find(|i| i.name == "child_n").unwrap();
        assert_eq!(idx.columns, ["n"]);
        assert_eq!(idx.predicate.as_deref(), Some("n > 1"));
        assert_eq!(cat.table("main", "v").unwrap().kind, TableKind::View);
    }
}
