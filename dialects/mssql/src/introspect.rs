//! Reads the SQL Server catalog views (`sys.*`) into a [`Catalog`].
//!
//! One query per object kind. [`assemble`] is pure so it is tested without a server. Needs SQL
//! Server 2012 or later (also Azure SQL).

use std::collections::HashMap;

use dg_dialect::{Connection, IntrospectScope, Result, ResultSet, Value};
use dg_model::{
    Catalog, Column, Constraint, ConstraintKind, FkAction, ForeignKeyTarget, Identity, Index,
    Routine, RoutineKind, Schema, Sequence, Table, TableKind,
};

fn literal(s: &str) -> String {
    format!("N'{}'", s.replace('\'', "''"))
}

/// Skips built-in schemas and honours the scope; `col` is a schema-name expression.
fn filter(col: &str, scope: &IntrospectScope) -> String {
    let mut out = format!(
        "{col} not in (N'sys', N'INFORMATION_SCHEMA', N'guest', N'db_owner', N'db_accessadmin', \
         N'db_securityadmin', N'db_ddladmin', N'db_backupoperator', N'db_datareader', \
         N'db_datawriter', N'db_denydatareader', N'db_denydatawriter')"
    );
    if let Some(only) = &scope.schemas {
        let list = only.iter().map(|s| literal(s)).collect::<Vec<_>>().join(", ");
        out.push_str(&format!(
            " and {col} in ({})",
            if list.is_empty() { "N''".into() } else { list }
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
    sequences: ResultSet,
    routines: ResultSet,
}

pub(crate) async fn introspect(
    conn: &mut dyn Connection,
    scope: &IntrospectScope,
) -> Result<Catalog> {
    let f = |c: &str| filter(c, scope);
    let desc = "(select cast(ep.value as nvarchar(max)) from sys.extended_properties ep \
                where ep.class = 1 and ep.name = N'MS_Description'";
    let raw = Raw {
        session: conn.query("select db_name(), schema_name()").await?,
        schemas: conn
            .query(&format!(
                "select s.name from sys.schemas s where s.schema_id < 16384 and {} order by s.name",
                f("s.name")
            ))
            .await?,
        tables: conn
            .query(&format!(
                "select s.name, o.name, o.type, convert(varchar(33), o.modify_date, 126), \
                 {desc} and ep.major_id = o.object_id and ep.minor_id = 0) \
                 from sys.objects o join sys.schemas s on s.schema_id = o.schema_id \
                 where o.type in ('U', 'V') and o.is_ms_shipped = 0 and {} order by s.name, o.name",
                f("s.name")
            ))
            .await?,
        columns: conn
            .query(&format!(
                "select s.name, o.name, c.name, t.name, c.max_length, c.precision, c.scale, \
                 c.is_nullable, object_definition(c.default_object_id), c.is_identity, \
                 cc.definition, {desc} and ep.major_id = c.object_id and ep.minor_id = c.column_id) \
                 from sys.columns c \
                 join sys.objects o on o.object_id = c.object_id \
                 join sys.schemas s on s.schema_id = o.schema_id \
                 join sys.types t on t.user_type_id = c.user_type_id \
                 left join sys.computed_columns cc on cc.object_id = c.object_id and cc.column_id = c.column_id \
                 where o.type in ('U', 'V') and o.is_ms_shipped = 0 and {} \
                 order by s.name, o.name, c.column_id",
                f("s.name")
            ))
            .await?,
        indexes: conn
            .query(&format!(
                "select s.name, o.name, i.name, i.is_unique, i.is_primary_key, i.is_unique_constraint, \
                 i.type_desc, i.filter_definition, c.name \
                 from sys.indexes i \
                 join sys.objects o on o.object_id = i.object_id \
                 join sys.schemas s on s.schema_id = o.schema_id \
                 join sys.index_columns ic on ic.object_id = i.object_id and ic.index_id = i.index_id \
                 join sys.columns c on c.object_id = ic.object_id and c.column_id = ic.column_id \
                 where i.type > 0 and ic.is_included_column = 0 and ic.key_ordinal > 0 \
                 and o.type in ('U', 'V') and o.is_ms_shipped = 0 and {} \
                 order by s.name, o.name, i.index_id, ic.key_ordinal",
                f("s.name")
            ))
            .await?,
        foreign_keys: conn
            .query(&format!(
                "select s.name, o.name, fk.name, pc.name, rs.name, ro.name, rc.name, \
                 fk.update_referential_action, fk.delete_referential_action \
                 from sys.foreign_keys fk \
                 join sys.foreign_key_columns fkc on fkc.constraint_object_id = fk.object_id \
                 join sys.objects o on o.object_id = fk.parent_object_id \
                 join sys.schemas s on s.schema_id = o.schema_id \
                 join sys.columns pc on pc.object_id = fkc.parent_object_id and pc.column_id = fkc.parent_column_id \
                 join sys.objects ro on ro.object_id = fk.referenced_object_id \
                 join sys.schemas rs on rs.schema_id = ro.schema_id \
                 join sys.columns rc on rc.object_id = fkc.referenced_object_id and rc.column_id = fkc.referenced_column_id \
                 where {} order by s.name, o.name, fk.name, fkc.constraint_column_id",
                f("s.name")
            ))
            .await?,
        checks: conn
            .query(&format!(
                "select s.name, o.name, cc.name, cc.definition \
                 from sys.check_constraints cc \
                 join sys.objects o on o.object_id = cc.parent_object_id \
                 join sys.schemas s on s.schema_id = o.schema_id \
                 where {} order by s.name, o.name, cc.name",
                f("s.name")
            ))
            .await?,
        // Sequences exist from SQL Server 2012; Synapse lacks them.
        sequences: conn
            .query(&format!(
                "select s.name, q.name, type_name(q.user_type_id), convert(varchar(40), q.start_value), \
                 convert(varchar(40), q.increment) \
                 from sys.sequences q join sys.schemas s on s.schema_id = q.schema_id \
                 where {} order by s.name, q.name",
                f("s.name")
            ))
            .await
            .unwrap_or_default(),
        routines: conn
            .query(&format!(
                "select s.name, o.name, o.type, \
                 (select top 1 case when p.max_length = -1 then type_name(p.user_type_id) + N'(max)' \
                         else type_name(p.user_type_id) end \
                  from sys.parameters p where p.object_id = o.object_id and p.parameter_id = 0), \
                 stuff((select N', ' + p.name + N' ' + type_name(p.user_type_id) \
                        from sys.parameters p where p.object_id = o.object_id and p.parameter_id > 0 \
                        order by p.parameter_id for xml path(N''), type).value('.', 'nvarchar(max)'), 1, 2, N'') \
                 from sys.objects o join sys.schemas s on s.schema_id = o.schema_id \
                 where o.type in ('P', 'FN', 'IF', 'TF', 'AF') and o.is_ms_shipped = 0 and {} \
                 order by s.name, o.name",
                f("s.name")
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

fn int(row: &[Value], i: usize) -> i64 {
    match row.get(i) {
        Some(Value::Int(n)) => *n,
        Some(Value::Bool(b)) => i64::from(*b),
        Some(Value::Text(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

fn flag(row: &[Value], i: usize) -> bool {
    int(row, i) != 0
}

fn action(code: i64) -> FkAction {
    match code {
        1 => FkAction::Cascade,
        2 => FkAction::SetNull,
        3 => FkAction::SetDefault,
        _ => FkAction::NoAction,
    }
}

/// `nvarchar(50)`, `decimal(10,2)`, `varchar(max)`, `datetime2(3)`.
fn native_type(name: &str, max_length: i64, precision: i64, scale: i64) -> String {
    match name {
        "char" | "varchar" | "binary" | "varbinary" => {
            if max_length < 0 {
                format!("{name}(max)")
            } else {
                format!("{name}({max_length})")
            }
        }
        "nchar" | "nvarchar" => {
            if max_length < 0 {
                format!("{name}(max)")
            } else {
                format!("{name}({})", max_length / 2)
            }
        }
        "decimal" | "numeric" => format!("{name}({precision},{scale})"),
        "datetime2" | "time" | "datetimeoffset" => format!("{name}({scale})"),
        _ => name.to_owned(),
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
    let schema_at = |schemas: &mut Vec<Schema>, name: &str| -> usize {
        schemas.iter().position(|s| s.name == name).unwrap_or_else(|| {
            schemas.push(Schema { name: name.to_owned(), ..Schema::default() });
            schemas.len() - 1
        })
    };
    let mut index: HashMap<(String, String), (usize, usize)> = HashMap::new();

    for r in &raw.tables.rows {
        let (Some(s), Some(t)) = (text(r, 0), text(r, 1)) else { continue };
        let si = schema_at(&mut schemas, &s);
        let kind = if text(r, 2).as_deref().map(str::trim) == Some("V") {
            TableKind::View
        } else {
            TableKind::Table
        };
        index.insert((s, t.clone()), (si, schemas[si].tables.len()));
        schemas[si].tables.push(Table {
            name: t,
            kind,
            comment: text(r, 4),
            state: text(r, 3),
            ..Table::default()
        });
    }

    for r in &raw.columns.rows {
        let (Some(s), Some(t), Some(name)) = (text(r, 0), text(r, 1), text(r, 2)) else { continue };
        let Some(&(si, ti)) = index.get(&(s, t)) else { continue };
        let computed = text(r, 10);
        schemas[si].tables[ti].columns.push(Column {
            name,
            native_type: native_type(
                &text(r, 3).unwrap_or_default(),
                int(r, 4),
                int(r, 5),
                int(r, 6),
            ),
            nullable: flag(r, 7),
            default: computed.clone().or_else(|| text(r, 8)),
            identity: flag(r, 9).then_some(Identity::ByDefault),
            generated: computed.is_some(),
            comment: text(r, 11),
        });
    }

    let mut last: Option<(String, String, String)> = None;
    for r in &raw.indexes.rows {
        let (Some(s), Some(t), Some(name), Some(col)) =
            (text(r, 0), text(r, 1), text(r, 2), text(r, 8))
        else {
            continue;
        };
        let Some(&(si, ti)) = index.get(&(s.clone(), t.clone())) else { continue };
        let key = (s, t, name.clone());
        let table = &mut schemas[si].tables[ti];
        if last.as_ref() == Some(&key) {
            if let Some(i) = table.indexes.last_mut() {
                i.columns.push(col);
            }
            continue;
        }
        last = Some(key);
        table.indexes.push(Index {
            primary: flag(r, 4),
            unique: flag(r, 3),
            method: text(r, 6).map(|m| m.to_ascii_lowercase()),
            name,
            columns: vec![col],
            predicate: text(r, 7),
        });
        // A unique constraint is also a unique index, but it is a constraint to the user.
        if flag(r, 5) {
            let i = table.indexes.last().cloned().unwrap_or_default();
            table.constraints.push(Constraint {
                name: i.name,
                kind: ConstraintKind::Unique,
                columns: Vec::new(),
                definition: String::new(),
                reference: None,
            });
        }
    }
    for schema in &mut schemas {
        for table in &mut schema.tables {
            let indexes = table.indexes.clone();
            // Fill in constraint columns now that every index row has been merged.
            for con in table.constraints.iter_mut().filter(|c| c.kind == ConstraintKind::Unique) {
                if let Some(i) = indexes.iter().find(|i| i.name == con.name) {
                    con.columns = i.columns.clone();
                    con.definition = format!("UNIQUE ({})", i.columns.join(", "));
                }
            }
            for i in indexes.iter().filter(|i| i.primary) {
                table.constraints.push(Constraint {
                    name: i.name.clone(),
                    kind: ConstraintKind::PrimaryKey,
                    definition: format!("PRIMARY KEY ({})", i.columns.join(", ")),
                    columns: i.columns.clone(),
                    reference: None,
                });
            }
            // PK and unique constraints show as constraints, not a second time as indexes.
            let constrained: Vec<String> = table
                .constraints
                .iter()
                .filter(|c| matches!(c.kind, ConstraintKind::PrimaryKey | ConstraintKind::Unique))
                .map(|c| c.name.clone())
                .collect();
            table.indexes.retain(|i| !constrained.contains(&i.name));
        }
    }

    let mut last: Option<(String, String, String)> = None;
    for r in &raw.foreign_keys.rows {
        let (Some(s), Some(t), Some(c), Some(col)) =
            (text(r, 0), text(r, 1), text(r, 2), text(r, 3))
        else {
            continue;
        };
        let (Some(rs), Some(rt), Some(rcol)) = (text(r, 4), text(r, 5), text(r, 6)) else {
            continue;
        };
        let Some(&(si, ti)) = index.get(&(s.clone(), t.clone())) else { continue };
        let key = (s, t, c.clone());
        let table = &mut schemas[si].tables[ti];
        if last.as_ref() == Some(&key) {
            if let Some(con) = table.constraints.last_mut() {
                con.columns.push(col);
                if let Some(target) = con.reference.as_mut() {
                    target.columns.push(rcol);
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
                schema: rs,
                table: rt,
                columns: vec![rcol],
                on_update: action(int(r, 7)),
                on_delete: action(int(r, 8)),
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
        let (Some(s), Some(t), Some(name), Some(def)) =
            (text(r, 0), text(r, 1), text(r, 2), text(r, 3))
        else {
            continue;
        };
        let Some(&(si, ti)) = index.get(&(s, t)) else { continue };
        schemas[si].tables[ti].constraints.push(Constraint {
            name,
            kind: ConstraintKind::Check,
            columns: Vec::new(),
            definition: format!("CHECK {def}"),
            reference: None,
        });
    }

    for r in &raw.sequences.rows {
        let (Some(s), Some(name)) = (text(r, 0), text(r, 1)) else { continue };
        let si = schema_at(&mut schemas, &s);
        schemas[si].sequences.push(Sequence {
            name,
            data_type: text(r, 2).unwrap_or_default(),
            start: text(r, 3),
            increment: text(r, 4),
        });
    }

    for r in &raw.routines.rows {
        let (Some(s), Some(name)) = (text(r, 0), text(r, 1)) else { continue };
        let si = schema_at(&mut schemas, &s);
        let kind = match text(r, 2).as_deref().map(str::trim) {
            Some("P") => RoutineKind::Procedure,
            Some("AF") => RoutineKind::Aggregate,
            _ => RoutineKind::Function,
        };
        schemas[si].routines.push(Routine {
            name,
            kind,
            arguments: text(r, 4).unwrap_or_default(),
            returns: (kind != RoutineKind::Procedure).then(|| text(r, 3)).flatten(),
        });
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

    fn rs(rows: Vec<Vec<Value>>) -> ResultSet {
        ResultSet { rows, ..ResultSet::default() }
    }

    fn t(s: &str) -> Value {
        if s == "~" { Value::Null } else { Value::Text(s.into()) }
    }

    fn row(cells: &[&str]) -> Vec<Value> {
        cells.iter().map(|c| t(c)).collect()
    }

    #[test]
    fn type_names_carry_their_parameters() {
        assert_eq!(native_type("nvarchar", 100, 0, 0), "nvarchar(50)");
        assert_eq!(native_type("nvarchar", -1, 0, 0), "nvarchar(max)");
        assert_eq!(native_type("varchar", 20, 0, 0), "varchar(20)");
        assert_eq!(native_type("decimal", 9, 10, 2), "decimal(10,2)");
        assert_eq!(native_type("datetime2", 8, 27, 3), "datetime2(3)");
        assert_eq!(native_type("int", 4, 10, 0), "int");
    }

    #[test]
    fn assembles_keys_indexes_and_routines() {
        let raw = Raw {
            session: rs(vec![row(&["shop", "dbo"])]),
            schemas: rs(vec![row(&["dbo"])]),
            tables: rs(vec![
                row(&["dbo", "orders", "U ", "2026-01-01", "~"]),
                row(&["dbo", "v", "V ", "~", "~"]),
            ]),
            columns: rs(vec![
                row(&["dbo", "orders", "id", "int", "4", "10", "0", "0", "~", "1", "~", "~"]),
                row(&[
                    "dbo",
                    "orders",
                    "note",
                    "nvarchar",
                    "200",
                    "0",
                    "0",
                    "1",
                    "~",
                    "0",
                    "~",
                    "free text",
                ]),
                row(&[
                    "dbo",
                    "orders",
                    "twice",
                    "int",
                    "4",
                    "10",
                    "0",
                    "1",
                    "~",
                    "0",
                    "([id]*(2))",
                    "~",
                ]),
            ]),
            indexes: rs(vec![
                row(&["dbo", "orders", "pk_orders", "1", "1", "0", "CLUSTERED", "~", "id"]),
                row(&["dbo", "orders", "uq_note", "1", "0", "1", "NONCLUSTERED", "~", "note"]),
                row(&[
                    "dbo",
                    "orders",
                    "ix_two",
                    "0",
                    "0",
                    "0",
                    "NONCLUSTERED",
                    "([id]>(1))",
                    "note",
                ]),
                row(&[
                    "dbo",
                    "orders",
                    "ix_two",
                    "0",
                    "0",
                    "0",
                    "NONCLUSTERED",
                    "([id]>(1))",
                    "id",
                ]),
            ]),
            foreign_keys: rs(vec![row(&[
                "dbo",
                "orders",
                "fk_c",
                "id",
                "dbo",
                "customers",
                "id",
                "0",
                "1",
            ])]),
            checks: rs(vec![row(&["dbo", "orders", "ck", "([id]>(0))"])]),
            sequences: rs(vec![row(&["dbo", "seq", "bigint", "1", "5"])]),
            routines: rs(vec![
                row(&["dbo", "f", "FN", "int", "@a int, @b int"]),
                row(&["dbo", "p", "P ", "~", "~"]),
            ]),
        };
        let cat = assemble(&raw);
        assert_eq!(cat.default_schema.as_deref(), Some("dbo"));
        let o = cat.table("dbo", "orders").unwrap();
        assert_eq!(o.primary_key(), ["id"]);
        assert_eq!(o.column("id").unwrap().identity, Some(Identity::ByDefault));
        assert_eq!(o.column("note").unwrap().native_type, "nvarchar(100)");
        assert!(o.column("twice").unwrap().generated);
        assert!(
            o.constraints.iter().any(|c| c.kind == ConstraintKind::Unique && c.columns == ["note"])
        );
        assert_eq!(o.indexes.len(), 1);
        assert_eq!(o.indexes[0].columns, ["note", "id"]);
        assert_eq!(o.indexes[0].predicate.as_deref(), Some("([id]>(1))"));
        let fk = o.foreign_keys().next().unwrap();
        assert_eq!(fk.reference.as_ref().unwrap().on_delete, FkAction::Cascade);
        assert!(o.constraints.iter().any(|c| c.kind == ConstraintKind::Check));
        assert_eq!(cat.table("dbo", "v").unwrap().kind, TableKind::View);
        let schema = cat.schema("dbo").unwrap();
        assert_eq!(schema.sequences[0].increment.as_deref(), Some("5"));
        assert_eq!(schema.routines[0].returns.as_deref(), Some("int"));
        assert_eq!(schema.routines[1].kind, RoutineKind::Procedure);
        assert!(schema.routines[1].returns.is_none());
    }

    #[test]
    fn filter_quotes_with_unicode_literals() {
        assert!(
            filter("s", &IntrospectScope { schemas: Some(vec!["it's".into()]) })
                .contains("s in (N'it''s')")
        );
    }
}
