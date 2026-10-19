//! Lists tables (`show tables`) and reads each one's key schema (`describe`) into a [`Catalog`].
//!
//! DynamoDB is schemaless apart from the key attributes, so those are the only columns shown.
//! The single schema is named after the region.

use rowcraft_dialect::{Connection, IntrospectScope, Result, ResultSet, Value};
use rowcraft_model::{Catalog, Column, Constraint, ConstraintKind, Index, Schema, Table};

/// Tables described in one introspection; the rest are listed with no key information.
const DESCRIBE_LIMIT: usize = 200;

pub(crate) async fn introspect(
    conn: &mut dyn Connection,
    scope: &IntrospectScope,
) -> Result<Catalog> {
    let names: Vec<String> =
        conn.query("show tables").await?.rows.iter().filter_map(|r| text(r.first()?)).collect();
    let mut tables = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let described = if i < DESCRIBE_LIMIT {
            conn.query(&format!("describe \"{name}\"")).await.ok()
        } else {
            None
        };
        tables.push(described.map_or_else(
            || Table { name: name.clone(), ..Table::default() },
            |d| assemble_table(name, &d),
        ));
    }
    // The region is not known here; `Catalog::database` carries nothing and the schema is "dynamodb".
    let schema = "dynamodb".to_owned();
    let schemas = if scope.schemas.as_ref().is_some_and(|s| !s.contains(&schema)) {
        Vec::new()
    } else {
        vec![Schema { name: schema.clone(), tables, ..Schema::default() }]
    };
    Ok(Catalog { database: None, default_schema: Some(schema), schemas })
}

fn text(v: &Value) -> Option<String> {
    match v {
        Value::Text(s) => Some(s.clone()),
        _ => None,
    }
}

fn type_name(code: &str) -> &'static str {
    match code {
        "S" => "string",
        "N" => "number",
        "B" => "binary",
        _ => "unknown",
    }
}

fn assemble_table(name: &str, describe: &ResultSet) -> Table {
    let mut table = Table { name: name.to_owned(), ..Table::default() };
    let mut keys: Vec<(String, String)> = Vec::new();
    for r in &describe.rows {
        let (Some(kind), Some(n), Some(t), detail) = (
            r.first().and_then(text),
            r.get(1).and_then(text),
            r.get(2).and_then(text),
            r.get(3).and_then(text).unwrap_or_default(),
        ) else {
            continue;
        };
        match kind.as_str() {
            "attribute" => table.columns.push(Column {
                name: n,
                native_type: type_name(&t).to_owned(),
                nullable: true,
                ..Column::default()
            }),
            "key" => keys.push((n, t)),
            "index" => table.indexes.push(Index {
                name: n,
                unique: false,
                primary: false,
                method: Some(t),
                columns: detail.split(',').filter(|c| !c.is_empty()).map(str::to_owned).collect(),
                predicate: None,
            }),
            _ => {}
        }
    }
    // Key attributes cannot be null; HASH comes before RANGE.
    keys.sort_by_key(|(_, t)| t != "HASH");
    for c in &mut table.columns {
        if keys.iter().any(|(k, _)| *k == c.name) {
            c.nullable = false;
        }
    }
    if !keys.is_empty() {
        let columns: Vec<String> = keys.into_iter().map(|(k, _)| k).collect();
        table.constraints.push(Constraint {
            name: "PRIMARY".into(),
            kind: ConstraintKind::PrimaryKey,
            definition: format!("PRIMARY KEY ({})", columns.join(", ")),
            columns,
            reference: None,
        });
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cells: &[&str]) -> Vec<Value> {
        cells.iter().map(|c| Value::Text((*c).into())).collect()
    }

    #[test]
    fn table_from_describe() {
        let d = ResultSet {
            rows: vec![
                row(&["attribute", "pk", "S", ""]),
                row(&["attribute", "ts", "N", ""]),
                row(&["key", "ts", "RANGE", ""]),
                row(&["key", "pk", "HASH", ""]),
                row(&["index", "by_ts", "GSI", "ts,pk"]),
            ],
            ..ResultSet::default()
        };
        let t = assemble_table("events", &d);
        assert_eq!(t.primary_key(), ["pk", "ts"]);
        assert_eq!(t.column("ts").unwrap().native_type, "number");
        assert!(!t.column("pk").unwrap().nullable);
        assert_eq!(t.indexes[0].columns, ["ts", "pk"]);
        assert_eq!(t.indexes[0].method.as_deref(), Some("GSI"));
    }
}
