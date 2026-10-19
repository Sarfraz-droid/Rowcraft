//! Reads `INFORMATION_SCHEMA` and `SHOW … KEYS` of the current database into a [`Catalog`].
//! Snowflake's keys are informational (not enforced) and are reported as metadata.

use std::collections::HashMap;

use rowcraft_dialect::{Connection, IntrospectScope, Result, ResultSet, Value};
use rowcraft_model::{
    Catalog, Column, Constraint, ConstraintKind, FkAction, ForeignKeyTarget, Identity, Schema,
    Table, TableKind,
};

type TableIdentity = (String, String);
type PrimaryKeyMetadata = (String, Vec<(i64, String)>);
type ForeignKeyIdentity = (String, String, String);
// Sequence, local column, referenced schema/table/column, update rule, delete rule.
type ForeignKeyColumnMetadata =
    (i64, String, String, String, String, Option<String>, Option<String>);

fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
}

fn filter(col: &str, scope: &IntrospectScope) -> String {
    let mut out = format!("{col} <> 'INFORMATION_SCHEMA'");
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
    primary: ResultSet,
    foreign: ResultSet,
}

pub(crate) async fn introspect(
    conn: &mut dyn Connection,
    scope: &IntrospectScope,
) -> Result<Catalog> {
    let raw = Raw {
        session: conn.query("select current_database(), current_schema()").await?,
        schemas: conn
            .query(&format!(
                "select schema_name, comment from information_schema.schemata where {} order by 1",
                filter("schema_name", scope)
            ))
            .await?,
        tables: conn
            .query(&format!(
                "select table_schema, table_name, table_type, comment, to_varchar(last_altered) \
                 from information_schema.tables where {} order by 1, 2",
                filter("table_schema", scope)
            ))
            .await?,
        columns: conn
            .query(&format!(
                "select table_schema, table_name, column_name, data_type, character_maximum_length, \
                 numeric_precision, numeric_scale, is_nullable, column_default, is_identity, comment \
                 from information_schema.columns where {} order by 1, 2, ordinal_position",
                filter("table_schema", scope)
            ))
            .await?,
        // Keys are optional: a role without access to them still gets tables and columns.
        primary: conn.query("show primary keys in database").await.unwrap_or_default(),
        foreign: conn.query("show imported keys in database").await.unwrap_or_default(),
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

/// Position of a column in a `SHOW` result, by name; the column order has changed over time.
fn at(rs: &ResultSet, name: &str) -> Option<usize> {
    rs.columns.iter().position(|c| c.eq_ignore_ascii_case(name))
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

fn native_type(r: &[Value]) -> String {
    let base = text(r, 3).unwrap_or_default();
    match base.as_str() {
        "TEXT" => text(r, 4).map_or(base, |l| format!("VARCHAR({l})")),
        "NUMBER" => match (text(r, 5), text(r, 6)) {
            (Some(p), Some(s)) => format!("NUMBER({p},{s})"),
            _ => base,
        },
        _ => base,
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
            Some("MATERIALIZED VIEW") => TableKind::MaterializedView,
            Some("EXTERNAL TABLE") => TableKind::ForeignTable,
            _ => TableKind::Table,
        };
        index.insert((s, t.clone()), (si, schemas[si].tables.len()));
        schemas[si].tables.push(Table {
            name: t,
            kind,
            comment: text(r, 3).filter(|c| !c.is_empty()),
            state: text(r, 4),
            ..Table::default()
        });
    }

    for r in &raw.columns.rows {
        let (Some(s), Some(t), Some(name)) = (text(r, 0), text(r, 1), text(r, 2)) else { continue };
        let Some(&(si, ti)) = index.get(&(s, t)) else { continue };
        schemas[si].tables[ti].columns.push(Column {
            name,
            native_type: native_type(r),
            nullable: text(r, 7).as_deref() != Some("NO"),
            default: text(r, 8),
            identity: (text(r, 9).as_deref() == Some("YES")).then_some(Identity::ByDefault),
            generated: false,
            comment: text(r, 10).filter(|c| !c.is_empty()),
        });
    }

    // SHOW PRIMARY KEYS: rows of one constraint, ordered by key_sequence.
    if let (Some(sc), Some(tb), Some(cl), Some(sq), Some(nm)) = (
        at(&raw.primary, "schema_name"),
        at(&raw.primary, "table_name"),
        at(&raw.primary, "column_name"),
        at(&raw.primary, "key_sequence"),
        at(&raw.primary, "constraint_name"),
    ) {
        let mut keys: HashMap<TableIdentity, PrimaryKeyMetadata> = HashMap::new();
        for r in &raw.primary.rows {
            let (Some(s), Some(t), Some(c)) = (text(r, sc), text(r, tb), text(r, cl)) else {
                continue;
            };
            let seq = text(r, sq).and_then(|n| n.parse().ok()).unwrap_or(0);
            let entry =
                keys.entry((s, t)).or_insert_with(|| (text(r, nm).unwrap_or_default(), Vec::new()));
            entry.1.push((seq, c));
        }
        for ((s, t), (name, mut cols)) in keys {
            let Some(&(si, ti)) = index.get(&(s, t)) else { continue };
            cols.sort();
            let columns: Vec<String> = cols.into_iter().map(|(_, c)| c).collect();
            schemas[si].tables[ti].constraints.push(Constraint {
                name,
                kind: ConstraintKind::PrimaryKey,
                definition: format!("PRIMARY KEY ({})", columns.join(", ")),
                columns,
                reference: None,
            });
        }
    }

    // SHOW IMPORTED KEYS: one row per column pair, grouped by foreign key name.
    let f = &raw.foreign;
    if let (
        Some(ps),
        Some(pt),
        Some(pc),
        Some(fs),
        Some(ft),
        Some(fc),
        Some(sq),
        Some(ur),
        Some(dr),
        Some(nm),
    ) = (
        at(f, "pk_schema_name"),
        at(f, "pk_table_name"),
        at(f, "pk_column_name"),
        at(f, "fk_schema_name"),
        at(f, "fk_table_name"),
        at(f, "fk_column_name"),
        at(f, "key_sequence"),
        at(f, "update_rule"),
        at(f, "delete_rule"),
        at(f, "fk_name"),
    ) {
        let mut order: Vec<ForeignKeyIdentity> = Vec::new();
        let mut parts: HashMap<ForeignKeyIdentity, Vec<ForeignKeyColumnMetadata>> = HashMap::new();
        for r in &f.rows {
            let (Some(s), Some(t), Some(c)) = (text(r, fs), text(r, ft), text(r, fc)) else {
                continue;
            };
            let key = (s, t, text(r, nm).unwrap_or_default());
            if !parts.contains_key(&key) {
                order.push(key.clone());
            }
            parts.entry(key).or_default().push((
                text(r, sq).and_then(|n| n.parse().ok()).unwrap_or(0),
                c,
                text(r, ps).unwrap_or_default(),
                text(r, pt).unwrap_or_default(),
                text(r, pc).unwrap_or_default(),
                text(r, ur),
                text(r, dr),
            ));
        }
        for key in order {
            let Some(mut rows) = parts.remove(&key) else { continue };
            rows.sort_by_key(|r| r.0);
            let Some(&(si, ti)) = index.get(&(key.0.clone(), key.1.clone())) else { continue };
            let columns: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
            let target_cols: Vec<String> = rows.iter().map(|r| r.4.clone()).collect();
            let (schema, table) = (rows[0].2.clone(), rows[0].3.clone());
            schemas[si].tables[ti].constraints.push(Constraint {
                name: key.2,
                kind: ConstraintKind::ForeignKey,
                definition: format!(
                    "FOREIGN KEY ({}) REFERENCES {schema}.{table} ({})",
                    columns.join(", "),
                    target_cols.join(", ")
                ),
                columns,
                reference: Some(ForeignKeyTarget {
                    schema,
                    table,
                    columns: target_cols,
                    on_update: action(rows[0].5.clone()),
                    on_delete: action(rows[0].6.clone()),
                }),
            });
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

    fn rs(columns: &[&str], rows: Vec<Vec<&str>>) -> ResultSet {
        ResultSet {
            columns: columns.iter().map(|c| (*c).to_owned()).collect(),
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
    fn assembles_tables_keys_and_foreign_keys() {
        let raw = Raw {
            session: rs(&[], vec![vec!["SHOP", "PUBLIC"]]),
            schemas: rs(&[], vec![vec!["PUBLIC", "~"]]),
            tables: rs(
                &[],
                vec![
                    vec!["PUBLIC", "ORDERS", "BASE TABLE", "~", "~"],
                    vec!["PUBLIC", "V", "VIEW", "~", "~"],
                ],
            ),
            columns: rs(
                &[],
                vec![
                    vec!["PUBLIC", "ORDERS", "ID", "NUMBER", "~", "38", "0", "NO", "~", "YES", "~"],
                    vec![
                        "PUBLIC", "ORDERS", "NOTE", "TEXT", "100", "~", "~", "YES", "~", "NO",
                        "memo",
                    ],
                    vec![
                        "PUBLIC", "ORDERS", "CID", "NUMBER", "~", "10", "2", "YES", "~", "NO", "~",
                    ],
                ],
            ),
            primary: rs(
                &[
                    "created_on",
                    "database_name",
                    "schema_name",
                    "table_name",
                    "column_name",
                    "key_sequence",
                    "constraint_name",
                ],
                vec![vec!["t", "SHOP", "PUBLIC", "ORDERS", "ID", "1", "PK_ORDERS"]],
            ),
            foreign: rs(
                &[
                    "pk_schema_name",
                    "pk_table_name",
                    "pk_column_name",
                    "fk_schema_name",
                    "fk_table_name",
                    "fk_column_name",
                    "key_sequence",
                    "update_rule",
                    "delete_rule",
                    "fk_name",
                ],
                vec![vec![
                    "PUBLIC",
                    "CUSTOMERS",
                    "ID",
                    "PUBLIC",
                    "ORDERS",
                    "CID",
                    "1",
                    "NO ACTION",
                    "CASCADE",
                    "FK_C",
                ]],
            ),
        };
        let cat = assemble(&raw);
        assert_eq!(cat.database.as_deref(), Some("SHOP"));
        let o = cat.table("PUBLIC", "ORDERS").unwrap();
        assert_eq!(o.primary_key(), ["ID"]);
        assert_eq!(o.column("ID").unwrap().native_type, "NUMBER(38,0)");
        assert_eq!(o.column("ID").unwrap().identity, Some(Identity::ByDefault));
        assert_eq!(o.column("NOTE").unwrap().native_type, "VARCHAR(100)");
        assert_eq!(o.column("NOTE").unwrap().comment.as_deref(), Some("memo"));
        let fk = o.foreign_keys().next().unwrap();
        assert_eq!(fk.reference.as_ref().unwrap().on_delete, FkAction::Cascade);
        assert_eq!(fk.definition, "FOREIGN KEY (CID) REFERENCES PUBLIC.CUSTOMERS (ID)");
        assert_eq!(cat.table("PUBLIC", "V").unwrap().kind, TableKind::View);
    }

    #[test]
    fn missing_show_results_are_fine() {
        let raw = Raw {
            session: rs(&[], vec![]),
            schemas: rs(&[], vec![vec!["PUBLIC", "~"]]),
            tables: rs(&[], vec![]),
            columns: rs(&[], vec![]),
            primary: ResultSet::default(),
            foreign: ResultSet::default(),
        };
        assert_eq!(assemble(&raw).schemas.len(), 1);
    }
}
