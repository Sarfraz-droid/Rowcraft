//! Reads MySQL / MariaDB `information_schema` into a [`Catalog`].
//!
//! In MySQL a schema is a database, so the catalog's schemas are the server's databases
//! (system ones skipped) and `default_schema` is `database()`. [`assemble`] is pure.

use std::collections::HashMap;

use dg_dialect::{Connection, IntrospectScope, Result, ResultSet, Value};
use dg_model::{
    Catalog, Column, Constraint, ConstraintKind, FkAction, ForeignKeyTarget, Identity, Index,
    Routine, RoutineKind, Schema, Table, TableKind,
};

use crate::Flavor;

const SYSTEM: &str = "'information_schema', 'mysql', 'performance_schema', 'sys'";

fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
}

fn filter(col: &str, scope: &IntrospectScope) -> String {
    let mut out = format!("{col} not in ({SYSTEM})");
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
    indexes: ResultSet,
    foreign_keys: ResultSet,
    checks: ResultSet,
    routines: ResultSet,
}

pub(crate) async fn introspect(
    conn: &mut dyn Connection,
    scope: &IntrospectScope,
    flavor: Flavor,
) -> Result<Catalog> {
    let expr = match flavor {
        Flavor::MySql => "coalesce(column_name, concat('(', expression, ')'))",
        Flavor::MariaDb => "column_name",
    };
    let raw = Raw {
        session: conn.query("select database(), database()").await?,
        schemas: conn
            .query(&format!(
                "select schema_name, null from information_schema.schemata where {} order by 1",
                filter("schema_name", scope)
            ))
            .await?,
        tables: conn
            .query(&format!(
                "select table_schema, table_name, table_type, table_comment, \
                 coalesce(update_time, create_time) \
                 from information_schema.tables where {} order by 1, 2",
                filter("table_schema", scope)
            ))
            .await?,
        columns: conn
            .query(&format!(
                "select table_schema, table_name, column_name, column_type, is_nullable, \
                 column_default, extra, column_comment, generation_expression \
                 from information_schema.columns where {} order by 1, 2, ordinal_position",
                filter("table_schema", scope)
            ))
            .await?,
        indexes: conn
            .query(&format!(
                "select table_schema, table_name, index_name, non_unique, index_type, {expr} \
                 from information_schema.statistics where {} order by 1, 2, 3, seq_in_index",
                filter("table_schema", scope)
            ))
            .await?,
        foreign_keys: conn
            .query(&format!(
                "select k.table_schema, k.table_name, k.constraint_name, k.column_name, \
                 k.referenced_table_schema, k.referenced_table_name, k.referenced_column_name, \
                 r.update_rule, r.delete_rule \
                 from information_schema.key_column_usage k \
                 join information_schema.referential_constraints r \
                   on r.constraint_schema = k.constraint_schema \
                  and r.constraint_name = k.constraint_name and r.table_name = k.table_name \
                 where k.referenced_table_name is not null and {} \
                 order by 1, 2, 3, k.ordinal_position",
                filter("k.table_schema", scope)
            ))
            .await?,
        // CHECK constraints exist from MySQL 8.0.16 / MariaDB 10.2; older servers have no view.
        checks: conn
            .query(&format!(
                "select tc.table_schema, tc.table_name, tc.constraint_name, cc.check_clause \
                 from information_schema.table_constraints tc \
                 join information_schema.check_constraints cc \
                   on cc.constraint_schema = tc.constraint_schema \
                  and cc.constraint_name = tc.constraint_name \
                 where tc.constraint_type = 'CHECK' and {} order by 1, 2, 3",
                filter("tc.table_schema", scope)
            ))
            .await
            .unwrap_or_default(),
        routines: conn
            .query(&format!(
                "select r.routine_schema, r.routine_name, r.routine_type, r.dtd_identifier, \
                 (select group_concat(concat(coalesce(p.parameter_mode, ''), ' ', p.parameter_name, ' ', p.dtd_identifier) \
                    order by p.ordinal_position separator ', ') \
                  from information_schema.parameters p \
                  where p.specific_schema = r.routine_schema and p.specific_name = r.specific_name \
                    and p.ordinal_position > 0) \
                 from information_schema.routines r where {} order by 1, 2",
                filter("r.routine_schema", scope)
            ))
            .await?,
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

fn action(s: Option<String>) -> FkAction {
    match s.as_deref() {
        Some("CASCADE") => FkAction::Cascade,
        Some("RESTRICT") => FkAction::Restrict,
        Some("SET NULL") => FkAction::SetNull,
        Some("SET DEFAULT") => FkAction::SetDefault,
        _ => FkAction::NoAction,
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
    let schema_at = |schemas: &mut Vec<Schema>, name: &str| -> usize {
        schemas.iter().position(|s| s.name == name).unwrap_or_else(|| {
            schemas.push(Schema { name: name.to_owned(), ..Schema::default() });
            schemas.len() - 1
        })
    };

    for r in &raw.tables.rows {
        let (Some(s), Some(t)) = (text(r, 0), text(r, 1)) else { continue };
        let si = schema_at(&mut schemas, &s);
        let kind = match text(r, 2).as_deref() {
            Some("VIEW") => TableKind::View,
            Some("SYSTEM VIEW") => TableKind::View,
            _ => TableKind::Table,
        };
        index.insert((s, t.clone()), (si, schemas[si].tables.len()));
        schemas[si].tables.push(Table {
            name: t,
            kind,
            // MySQL fills a view's comment with the word VIEW.
            comment: text(r, 3)
                .filter(|c| !c.is_empty() && !(kind == TableKind::View && c == "VIEW")),
            state: text(r, 4),
            ..Table::default()
        });
    }

    for r in &raw.columns.rows {
        let (Some(s), Some(t), Some(name)) = (text(r, 0), text(r, 1), text(r, 2)) else { continue };
        let Some(&(si, ti)) = index.get(&(s, t)) else { continue };
        let extra = text(r, 6).unwrap_or_default().to_ascii_lowercase();
        let generated = extra.contains("generated");
        // A generated column keeps its expression where other columns keep their default.
        let default = if generated { text(r, 8).filter(|e| !e.is_empty()) } else { text(r, 5) };
        schemas[si].tables[ti].columns.push(Column {
            name,
            native_type: text(r, 3).unwrap_or_default(),
            nullable: text(r, 4).as_deref() != Some("NO"),
            default,
            identity: extra.contains("auto_increment").then_some(Identity::ByDefault),
            generated,
            comment: text(r, 7).filter(|c| !c.is_empty()),
        });
    }

    // Indexes: consecutive rows of one (table, index) are its columns in key order.
    let mut last: Option<(String, String, String)> = None;
    for r in &raw.indexes.rows {
        let (Some(s), Some(t), Some(name)) = (text(r, 0), text(r, 1), text(r, 2)) else { continue };
        let Some(&(si, ti)) = index.get(&(s.clone(), t.clone())) else { continue };
        let column = text(r, 5).unwrap_or_default();
        let key = (s, t, name.clone());
        let table = &mut schemas[si].tables[ti];
        if last.as_ref() == Some(&key) {
            if let Some(i) = table.indexes.last_mut() {
                i.columns.push(column);
            }
            continue;
        }
        last = Some(key);
        table.indexes.push(Index {
            primary: name == "PRIMARY",
            unique: text(r, 3).as_deref() == Some("0"),
            method: text(r, 4).map(|m| m.to_ascii_lowercase()),
            name,
            columns: vec![column],
            predicate: None,
        });
    }
    // PRIMARY KEY and UNIQUE constraints are the unique indexes MySQL builds for them.
    for schema in &mut schemas {
        for table in &mut schema.tables {
            let indexes = table.indexes.clone();
            for i in indexes.iter().filter(|i| i.unique) {
                let (kind, label) = if i.primary {
                    (ConstraintKind::PrimaryKey, "PRIMARY KEY")
                } else {
                    (ConstraintKind::Unique, "UNIQUE")
                };
                table.constraints.push(Constraint {
                    name: i.name.clone(),
                    kind,
                    definition: format!("{label} ({})", i.columns.join(", ")),
                    columns: i.columns.clone(),
                    reference: None,
                });
            }
            // The PK is shown as a constraint, not a second time as an index.
            table.indexes.retain(|i| !i.primary);
        }
    }

    let mut last: Option<(String, String, String)> = None;
    for r in &raw.foreign_keys.rows {
        let (Some(s), Some(t), Some(c), Some(col)) =
            (text(r, 0), text(r, 1), text(r, 2), text(r, 3))
        else {
            continue;
        };
        let (Some(ps), Some(pt), Some(pcol)) = (text(r, 4), text(r, 5), text(r, 6)) else {
            continue;
        };
        let Some(&(si, ti)) = index.get(&(s.clone(), t.clone())) else { continue };
        let key = (s, t, c.clone());
        let table = &mut schemas[si].tables[ti];
        if last.as_ref() == Some(&key) {
            if let Some(con) = table.constraints.last_mut() {
                con.columns.push(col);
                if let Some(target) = con.reference.as_mut() {
                    target.columns.push(pcol);
                }
            }
            continue;
        }
        last = Some(key);
        table.constraints.push(Constraint {
            name: c,
            kind: ConstraintKind::ForeignKey,
            columns: vec![col],
            definition: String::new(),
            reference: Some(ForeignKeyTarget {
                schema: ps,
                table: pt,
                columns: vec![pcol],
                on_update: action(text(r, 7)),
                on_delete: action(text(r, 8)),
            }),
        });
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

    for r in &raw.checks.rows {
        let (Some(s), Some(t), Some(name), Some(clause)) =
            (text(r, 0), text(r, 1), text(r, 2), text(r, 3))
        else {
            continue;
        };
        let Some(&(si, ti)) = index.get(&(s, t)) else { continue };
        schemas[si].tables[ti].constraints.push(Constraint {
            name,
            kind: ConstraintKind::Check,
            columns: Vec::new(),
            definition: format!("CHECK ({clause})"),
            reference: None,
        });
    }

    for r in &raw.routines.rows {
        let (Some(s), Some(name)) = (text(r, 0), text(r, 1)) else { continue };
        let si = schema_at(&mut schemas, &s);
        let kind = if text(r, 2).as_deref() == Some("PROCEDURE") {
            RoutineKind::Procedure
        } else {
            RoutineKind::Function
        };
        schemas[si].routines.push(Routine {
            name,
            kind,
            arguments: text(r, 4).unwrap_or_default(),
            returns: (kind == RoutineKind::Function).then(|| text(r, 3)).flatten(),
        });
    }

    let session = raw.session.rows.first();
    let db = session.and_then(|r| text(r, 0));
    Catalog { database: db.clone(), default_schema: db, schemas }
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
    fn assembles_a_mysql_schema() {
        let raw = Raw {
            session: rs(vec![vec!["shop", "shop"]]),
            schemas: rs(vec![vec!["shop", "~"]]),
            tables: rs(vec![
                vec!["shop", "orders", "BASE TABLE", "", "~"],
                vec!["shop", "v", "VIEW", "~", "~"],
            ]),
            columns: rs(vec![
                vec!["shop", "orders", "id", "int", "NO", "~", "auto_increment", "", ""],
                vec!["shop", "orders", "total", "decimal(10,2)", "YES", "0.00", "", "money", ""],
                vec![
                    "shop",
                    "orders",
                    "gross",
                    "decimal(10,2)",
                    "YES",
                    "~",
                    "STORED GENERATED",
                    "",
                    "`total` * 2",
                ],
            ]),
            indexes: rs(vec![
                vec!["shop", "orders", "PRIMARY", "0", "BTREE", "id"],
                vec!["shop", "orders", "by_total", "1", "BTREE", "total"],
                vec!["shop", "orders", "by_total", "1", "BTREE", "id"],
            ]),
            foreign_keys: rs(vec![vec![
                "shop",
                "orders",
                "fk_c",
                "id",
                "shop",
                "customers",
                "id",
                "CASCADE",
                "RESTRICT",
            ]]),
            checks: rs(vec![vec!["shop", "orders", "pos", "(`total` >= 0)"]]),
            routines: rs(vec![vec![
                "shop",
                "tax",
                "FUNCTION",
                "decimal(10,2)",
                "IN x decimal(10,2)",
            ]]),
        };
        let cat = assemble(&raw);
        let t = cat.table("shop", "orders").unwrap();
        assert_eq!(t.primary_key(), ["id"]);
        assert_eq!(t.column("id").unwrap().identity, Some(Identity::ByDefault));
        assert!(!t.column("id").unwrap().nullable);
        assert_eq!(t.column("total").unwrap().comment.as_deref(), Some("money"));
        let g = t.column("gross").unwrap();
        assert!(g.generated);
        assert_eq!(g.default.as_deref(), Some("`total` * 2"));
        assert_eq!(t.indexes.len(), 1);
        assert_eq!(t.indexes[0].columns, ["total", "id"]);
        assert!(!t.indexes[0].unique);
        let fk = t.foreign_keys().next().unwrap();
        assert_eq!(fk.reference.as_ref().unwrap().on_delete, FkAction::Restrict);
        assert!(t.constraints.iter().any(|c| c.kind == ConstraintKind::Check));
        assert_eq!(cat.table("shop", "v").unwrap().kind, TableKind::View);
        let r = &cat.schema("shop").unwrap().routines[0];
        assert_eq!(r.returns.as_deref(), Some("decimal(10,2)"));
    }

    #[test]
    fn filter_quotes_names() {
        let f = filter("s", &IntrospectScope { schemas: Some(vec!["a'b".into()]) });
        assert!(f.contains("s in ('a''b')"));
    }
}
