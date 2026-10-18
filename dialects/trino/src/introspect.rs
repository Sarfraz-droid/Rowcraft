//! Reads `information_schema` of each catalog into a [`Catalog`]. With several catalogs a schema
//! is named `catalog.schema`; with one (the session's) it is just the schema name.

use dg_dialect::{Connection, IntrospectScope, Result, ResultSet, Value};
use dg_model::{Catalog, Column, Schema, Table, TableKind};

/// Catalogs that hold server internals rather than data.
const SKIPPED: &[&str] = &["system", "jmx"];

fn quote(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

pub(crate) async fn introspect(
    conn: &mut dyn Connection,
    scope: &IntrospectScope,
) -> Result<Catalog> {
    let session = conn.query("select current_catalog, current_schema").await?;
    let current = session.rows.first().and_then(|r| text(r, 0));
    let default_schema = session.rows.first().and_then(|r| text(r, 1));
    let catalogs: Vec<String> = match &current {
        Some(c) => vec![c.clone()],
        None => conn
            .query("show catalogs")
            .await?
            .rows
            .iter()
            .filter_map(|r| text(r, 0))
            .filter(|c| !SKIPPED.contains(&c.as_str()))
            .collect(),
    };
    let multiple = catalogs.len() > 1;
    let mut schemas: Vec<Schema> = Vec::new();
    for catalog in &catalogs {
        let name = |s: &str| if multiple { format!("{catalog}.{s}") } else { s.to_owned() };
        let q = quote(catalog);
        let tables = conn
            .query(&format!(
                "select table_schema, table_name, table_type from {q}.information_schema.tables \
                 where table_schema <> 'information_schema' order by 1, 2"
            ))
            .await;
        let columns = conn
            .query(&format!(
                "select table_schema, table_name, column_name, data_type, is_nullable, column_default \
                 from {q}.information_schema.columns where table_schema <> 'information_schema' \
                 order by table_schema, table_name, ordinal_position"
            ))
            .await;
        // A catalog the user may not read (or one that errors) is skipped, not fatal.
        let (Ok(tables), Ok(columns)) = (tables, columns) else { continue };
        assemble(catalog, &name, &tables, &columns, scope, &mut schemas);
    }
    Ok(Catalog { database: current, default_schema, schemas })
}

fn text(row: &[Value], i: usize) -> Option<String> {
    match row.get(i)? {
        Value::Null => None,
        Value::Text(s) => Some(s.clone()),
        Value::Int(n) => Some(n.to_string()),
        other => Some(format!("{other:?}")),
    }
}

fn assemble(
    _catalog: &str,
    name: &dyn Fn(&str) -> String,
    tables: &ResultSet,
    columns: &ResultSet,
    scope: &IntrospectScope,
    schemas: &mut Vec<Schema>,
) {
    let mut at: std::collections::HashMap<(String, String), (usize, usize)> = Default::default();
    for r in &tables.rows {
        let (Some(s), Some(t)) = (text(r, 0), text(r, 1)) else { continue };
        let schema = name(&s);
        if scope.schemas.as_ref().is_some_and(|only| !only.contains(&schema)) {
            continue;
        }
        let si = match schemas.iter().position(|x| x.name == schema) {
            Some(i) => i,
            None => {
                schemas.push(Schema { name: schema.clone(), ..Schema::default() });
                schemas.len() - 1
            }
        };
        let kind =
            if text(r, 2).as_deref() == Some("VIEW") { TableKind::View } else { TableKind::Table };
        at.insert((s, t.clone()), (si, schemas[si].tables.len()));
        schemas[si].tables.push(Table { name: t, kind, ..Table::default() });
    }
    for r in &columns.rows {
        let (Some(s), Some(t), Some(c)) = (text(r, 0), text(r, 1), text(r, 2)) else { continue };
        let Some(&(si, ti)) = at.get(&(s, t)) else { continue };
        schemas[si].tables[ti].columns.push(Column {
            name: c,
            native_type: text(r, 3).unwrap_or_default(),
            nullable: text(r, 4).as_deref() != Some("NO"),
            default: text(r, 5),
            ..Column::default()
        });
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
    fn schemas_are_prefixed_when_several_catalogs_are_shown() {
        let mut schemas = Vec::new();
        let tables = rs(vec![vec!["tiny", "orders", "BASE TABLE"], vec!["tiny", "v", "VIEW"]]);
        let columns = rs(vec![
            vec!["tiny", "orders", "id", "bigint", "NO", "~"],
            vec!["tiny", "orders", "note", "varchar(10)", "YES", "~"],
        ]);
        assemble(
            "tpch",
            &|s| format!("tpch.{s}"),
            &tables,
            &columns,
            &IntrospectScope::default(),
            &mut schemas,
        );
        assert_eq!(schemas[0].name, "tpch.tiny");
        let orders = schemas[0].table("orders").unwrap();
        assert!(!orders.column("id").unwrap().nullable);
        assert_eq!(orders.column("note").unwrap().native_type, "varchar(10)");
        assert_eq!(schemas[0].table("v").unwrap().kind, TableKind::View);
    }

    #[test]
    fn scope_filters_by_displayed_name() {
        let mut schemas = Vec::new();
        let tables = rs(vec![vec!["a", "t", "BASE TABLE"], vec!["b", "t", "BASE TABLE"]]);
        assemble(
            "c",
            &|s| s.to_owned(),
            &tables,
            &ResultSet::default(),
            &IntrospectScope { schemas: Some(vec!["b".into()]) },
            &mut schemas,
        );
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].name, "b");
    }
}
