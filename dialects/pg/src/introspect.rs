//! Reads the PostgreSQL catalog into a [`Catalog`].
//!
//! One query per object kind, all inside a single repeatable-read transaction so the pieces agree
//! with each other. Rows come back as text (simple query protocol) and [`assemble`] turns them
//! into the model; that step is pure so it can be tested without a server.
//!
//! Supports PostgreSQL 12 and later (`attgenerated`, `prokind`, `indnkeyatts`).

use std::collections::HashMap;

use rowcraft_dialect::{Connection, Error, IntrospectScope, Result, ResultSet, Value};
use rowcraft_model::{
    Catalog, Column, Constraint, ConstraintKind, FkAction, ForeignKeyTarget, Identity, Index,
    Routine, RoutineKind, Schema, Sequence, Table, TableKind,
};

/// Separates names inside an aggregated column; cannot appear in a sane identifier.
const SEP: char = '\u{1f}';

pub(crate) async fn introspect(
    conn: &mut dyn Connection,
    scope: &IntrospectScope,
) -> Result<Catalog> {
    conn.query("begin transaction isolation level repeatable read read only").await?;
    let raw = fetch(conn, scope).await;
    // Read-only, so commit and rollback are equivalent; rollback is the safe one after an error.
    conn.query(if raw.is_ok() { "commit" } else { "rollback" }).await?;
    assemble(&raw?)
}

struct Raw {
    /// `current_database()`, `current_schema()`.
    session: ResultSet,
    schemas: ResultSet,
    tables: ResultSet,
    columns: ResultSet,
    indexes: ResultSet,
    constraints: ResultSet,
    sequences: ResultSet,
    routines: ResultSet,
}

async fn fetch(conn: &mut dyn Connection, scope: &IntrospectScope) -> Result<Raw> {
    Ok(Raw {
        session: conn.query("select current_database(), current_schema()").await?,
        schemas: conn.query(&schemas_sql(scope)).await?,
        tables: conn.query(&tables_sql(scope)).await?,
        columns: conn.query(&columns_sql(scope)).await?,
        indexes: conn.query(&indexes_sql(scope)).await?,
        constraints: conn.query(&constraints_sql(scope)).await?,
        sequences: conn.query(&sequences_sql(scope)).await?,
        routines: conn.query(&routines_sql(scope)).await?,
    })
}

/// `E'...'` literal that is safe whatever `standard_conforming_strings` says.
fn literal(s: &str) -> String {
    format!("E'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
}

/// WHERE fragment limiting `col` (a schema name expression) to user schemas in scope.
fn schema_filter(col: &str, scope: &IntrospectScope) -> String {
    let mut sql = format!(
        "{col} not in ('pg_catalog', 'information_schema') and {col} !~ '^pg_(toast|temp)'"
    );
    if let Some(list) = &scope.schemas {
        if list.is_empty() {
            sql.push_str(" and false");
        } else {
            let names: Vec<_> = list.iter().map(|s| literal(s)).collect();
            sql.push_str(&format!(" and {col} in ({})", names.join(", ")));
        }
    }
    sql
}

/// Objects installed by an extension (PostGIS tables, …) are noise in the explorer.
fn not_extension_member(class: &str, oid: &str) -> String {
    format!(
        "not exists (select 1 from pg_catalog.pg_depend d where d.classid = '{class}'::regclass \
         and d.objid = {oid} and d.deptype = 'e')"
    )
}

fn schemas_sql(scope: &IntrospectScope) -> String {
    format!(
        "select n.nspname, pg_catalog.obj_description(n.oid, 'pg_namespace') \
         from pg_catalog.pg_namespace n where {} order by 1",
        schema_filter("n.nspname", scope)
    )
}

fn tables_sql(scope: &IntrospectScope) -> String {
    format!(
        "select n.nspname, c.relname, c.relkind::text, \
                pg_catalog.obj_description(c.oid, 'pg_class'), c.xmin::text \
         from pg_catalog.pg_class c join pg_catalog.pg_namespace n on n.oid = c.relnamespace \
         where c.relkind in ('r', 'p', 'v', 'm', 'f') and {} and {} order by 1, 2",
        schema_filter("n.nspname", scope),
        not_extension_member("pg_class", "c.oid")
    )
}

fn columns_sql(scope: &IntrospectScope) -> String {
    format!(
        "select n.nspname, c.relname, a.attname, \
                pg_catalog.format_type(a.atttypid, a.atttypmod), a.attnotnull::text, \
                pg_catalog.pg_get_expr(d.adbin, d.adrelid), \
                a.attidentity::text, a.attgenerated::text, \
                pg_catalog.col_description(c.oid, a.attnum) \
         from pg_catalog.pg_attribute a \
              join pg_catalog.pg_class c on c.oid = a.attrelid \
              join pg_catalog.pg_namespace n on n.oid = c.relnamespace \
              left join pg_catalog.pg_attrdef d on d.adrelid = a.attrelid and d.adnum = a.attnum \
         where a.attnum > 0 and not a.attisdropped \
           and c.relkind in ('r', 'p', 'v', 'm', 'f') and {} and {} \
         order by 1, 2, a.attnum",
        schema_filter("n.nspname", scope),
        not_extension_member("pg_class", "c.oid")
    )
}

fn indexes_sql(scope: &IntrospectScope) -> String {
    format!(
        "select n.nspname, t.relname, i.relname, x.indisunique::text, x.indisprimary::text, \
                am.amname, pg_catalog.pg_get_expr(x.indpred, x.indrelid), k, \
                pg_catalog.pg_get_indexdef(x.indexrelid, k, true) \
         from pg_catalog.pg_index x \
              join pg_catalog.pg_class i on i.oid = x.indexrelid \
              join pg_catalog.pg_class t on t.oid = x.indrelid \
              join pg_catalog.pg_namespace n on n.oid = t.relnamespace \
              join pg_catalog.pg_am am on am.oid = i.relam \
              cross join generate_series(1, x.indnkeyatts::int) k \
         where t.relkind in ('r', 'p', 'm') and {} and {} \
         order by 1, 2, 3, k",
        schema_filter("n.nspname", scope),
        not_extension_member("pg_class", "t.oid")
    )
}

fn constraints_sql(scope: &IntrospectScope) -> String {
    format!(
        "select n.nspname, t.relname, c.conname, c.contype::text, \
                pg_catalog.pg_get_constraintdef(c.oid, true), \
                (select string_agg(a.attname, chr(31) order by u.ord) \
                   from unnest(c.conkey) with ordinality u(attnum, ord) \
                   join pg_catalog.pg_attribute a \
                     on a.attrelid = c.conrelid and a.attnum = u.attnum), \
                rn.nspname, rt.relname, \
                (select string_agg(a.attname, chr(31) order by u.ord) \
                   from unnest(c.confkey) with ordinality u(attnum, ord) \
                   join pg_catalog.pg_attribute a \
                     on a.attrelid = c.confrelid and a.attnum = u.attnum), \
                c.confupdtype::text, c.confdeltype::text \
         from pg_catalog.pg_constraint c \
              join pg_catalog.pg_class t on t.oid = c.conrelid \
              join pg_catalog.pg_namespace n on n.oid = t.relnamespace \
              left join pg_catalog.pg_class rt on rt.oid = c.confrelid \
              left join pg_catalog.pg_namespace rn on rn.oid = rt.relnamespace \
         where c.contype in ('p', 'u', 'f', 'c', 'x') and {} and {} \
         order by 1, 2, 3",
        schema_filter("n.nspname", scope),
        not_extension_member("pg_class", "t.oid")
    )
}

fn sequences_sql(scope: &IntrospectScope) -> String {
    // deptype 'i' marks the hidden sequence behind an identity column.
    format!(
        "select n.nspname, c.relname, pg_catalog.format_type(s.seqtypid, null), \
                s.seqstart::text, s.seqincrement::text \
         from pg_catalog.pg_sequence s \
              join pg_catalog.pg_class c on c.oid = s.seqrelid \
              join pg_catalog.pg_namespace n on n.oid = c.relnamespace \
         where {} and not exists (select 1 from pg_catalog.pg_depend d \
                 where d.classid = 'pg_class'::regclass and d.objid = c.oid \
                   and d.deptype in ('i', 'e')) \
         order by 1, 2",
        schema_filter("n.nspname", scope)
    )
}

fn routines_sql(scope: &IntrospectScope) -> String {
    format!(
        "select n.nspname, p.proname, p.prokind::text, \
                pg_catalog.pg_get_function_arguments(p.oid), \
                case when p.prokind = 'p' then null \
                     else pg_catalog.pg_get_function_result(p.oid) end \
         from pg_catalog.pg_proc p join pg_catalog.pg_namespace n on n.oid = p.pronamespace \
         where {} and {} order by 1, 2, 4",
        schema_filter("n.nspname", scope),
        not_extension_member("pg_proc", "p.oid")
    )
}

fn text(row: &[Value], i: usize) -> Option<&str> {
    match row.get(i) {
        Some(Value::Text(s)) => Some(s),
        _ => None,
    }
}

fn string(row: &[Value], i: usize) -> String {
    text(row, i).unwrap_or_default().to_owned()
}

fn flag(row: &[Value], i: usize) -> bool {
    matches!(text(row, i), Some("t" | "true"))
}

fn names(row: &[Value], i: usize) -> Vec<String> {
    text(row, i)
        .filter(|s| !s.is_empty())
        .map_or_else(Vec::new, |s| s.split(SEP).map(str::to_owned).collect())
}

fn non_empty(row: &[Value], i: usize) -> Option<String> {
    text(row, i).filter(|s| !s.is_empty()).map(str::to_owned)
}

fn table_kind(code: &str) -> Option<TableKind> {
    Some(match code {
        "r" => TableKind::Table,
        "p" => TableKind::PartitionedTable,
        "v" => TableKind::View,
        "m" => TableKind::MaterializedView,
        "f" => TableKind::ForeignTable,
        _ => return None,
    })
}

fn fk_action(code: Option<&str>) -> FkAction {
    match code {
        Some("r") => FkAction::Restrict,
        Some("c") => FkAction::Cascade,
        Some("n") => FkAction::SetNull,
        Some("d") => FkAction::SetDefault,
        _ => FkAction::NoAction,
    }
}

fn constraint_kind(code: &str) -> Option<ConstraintKind> {
    Some(match code {
        "p" => ConstraintKind::PrimaryKey,
        "u" => ConstraintKind::Unique,
        "f" => ConstraintKind::ForeignKey,
        "c" => ConstraintKind::Check,
        "x" => ConstraintKind::Exclusion,
        _ => return None,
    })
}

fn routine_kind(code: &str) -> RoutineKind {
    match code {
        "p" => RoutineKind::Procedure,
        "a" => RoutineKind::Aggregate,
        "w" => RoutineKind::Window,
        _ => RoutineKind::Function,
    }
}

fn bad(what: &str, rs: &ResultSet, want: usize) -> Error {
    Error::Query(format!(
        "introspection: {what} query returned {} columns, expected {want}",
        rs.columns.len()
    ))
}

fn check_width(raw: &Raw) -> Result<()> {
    for (what, rs, want) in [
        ("session", &raw.session, 2),
        ("schemas", &raw.schemas, 2),
        ("tables", &raw.tables, 5),
        ("columns", &raw.columns, 9),
        ("indexes", &raw.indexes, 9),
        ("constraints", &raw.constraints, 11),
        ("sequences", &raw.sequences, 5),
        ("routines", &raw.routines, 5),
    ] {
        if rs.columns.len() != want {
            return Err(bad(what, rs, want));
        }
    }
    Ok(())
}

/// Finds `(schema index, table index)` for a row's `(schema, table)` pair.
type TableIndex = HashMap<(String, String), (usize, usize)>;

fn assemble(raw: &Raw) -> Result<Catalog> {
    check_width(raw)?;
    let mut catalog = Catalog::default();
    if let Some(row) = raw.session.rows.first() {
        catalog.database = non_empty(row, 0);
        catalog.default_schema = non_empty(row, 1);
    }
    let mut schema_at: HashMap<String, usize> = HashMap::new();
    for row in &raw.schemas.rows {
        schema_at.insert(string(row, 0), catalog.schemas.len());
        catalog.schemas.push(Schema {
            name: string(row, 0),
            comment: non_empty(row, 1),
            ..Schema::default()
        });
    }

    let mut tables: TableIndex = HashMap::new();
    for row in &raw.tables.rows {
        let (Some(&si), Some(kind)) =
            (schema_at.get(&string(row, 0)), table_kind(text(row, 2).unwrap_or_default()))
        else {
            continue;
        };
        let schema = &mut catalog.schemas[si];
        tables.insert((string(row, 0), string(row, 1)), (si, schema.tables.len()));
        schema.tables.push(Table {
            name: string(row, 1),
            kind,
            comment: non_empty(row, 3),
            state: non_empty(row, 4),
            ..Table::default()
        });
    }

    for row in &raw.columns.rows {
        with_table(&mut catalog, &tables, row, |t| {
            t.columns.push(Column {
                name: string(row, 2),
                native_type: string(row, 3),
                nullable: !flag(row, 4),
                default: non_empty(row, 5),
                identity: match text(row, 6) {
                    Some("a") => Some(Identity::Always),
                    Some("d") => Some(Identity::ByDefault),
                    _ => None,
                },
                generated: text(row, 7) == Some("s"),
                comment: non_empty(row, 8),
            });
        });
    }

    // One row per index key column, ordered by (index, position): fold into indexes.
    for row in &raw.indexes.rows {
        with_table(&mut catalog, &tables, row, |t| {
            let name = string(row, 2);
            let column = string(row, 8);
            match t.indexes.last_mut() {
                Some(ix) if ix.name == name => ix.columns.push(column),
                _ => t.indexes.push(Index {
                    name,
                    unique: flag(row, 3),
                    primary: flag(row, 4),
                    method: non_empty(row, 5),
                    columns: vec![column],
                    predicate: non_empty(row, 6),
                }),
            }
        });
    }

    for row in &raw.constraints.rows {
        let Some(kind) = constraint_kind(text(row, 3).unwrap_or_default()) else {
            continue;
        };
        with_table(&mut catalog, &tables, row, |t| {
            let reference = (kind == ConstraintKind::ForeignKey).then(|| ForeignKeyTarget {
                schema: string(row, 6),
                table: string(row, 7),
                columns: names(row, 8),
                on_update: fk_action(text(row, 9)),
                on_delete: fk_action(text(row, 10)),
            });
            t.constraints.push(Constraint {
                name: string(row, 2),
                kind,
                columns: names(row, 5),
                definition: string(row, 4),
                reference,
            });
        });
    }

    for row in &raw.sequences.rows {
        if let Some(&si) = schema_at.get(&string(row, 0)) {
            catalog.schemas[si].sequences.push(Sequence {
                name: string(row, 1),
                data_type: string(row, 2),
                start: non_empty(row, 3),
                increment: non_empty(row, 4),
            });
        }
    }

    for row in &raw.routines.rows {
        if let Some(&si) = schema_at.get(&string(row, 0)) {
            catalog.schemas[si].routines.push(Routine {
                name: string(row, 1),
                kind: routine_kind(text(row, 2).unwrap_or_default()),
                arguments: string(row, 3),
                returns: non_empty(row, 4),
            });
        }
    }
    Ok(catalog)
}

/// Runs `f` on the table named by the row's first two cells. A row for an unknown table (an
/// object filtered out, or dropped between queries) is skipped.
fn with_table(
    catalog: &mut Catalog,
    tables: &TableIndex,
    row: &[Value],
    f: impl FnOnce(&mut Table),
) {
    if let Some(&(si, ti)) = tables.get(&(string(row, 0), string(row, 1))) {
        f(&mut catalog.schemas[si].tables[ti]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rs(width: usize, rows: &[&[Option<&str>]]) -> ResultSet {
        ResultSet {
            columns: (0..width).map(|i| format!("c{i}")).collect(),
            rows: rows
                .iter()
                .map(|r| {
                    r.iter().map(|c| c.map_or(Value::Null, |s| Value::Text(s.to_owned()))).collect()
                })
                .collect(),
            ..ResultSet::default()
        }
    }

    fn sample() -> Raw {
        let s = Some;
        Raw {
            session: rs(2, &[&[s("app"), s("public")]]),
            schemas: rs(2, &[&[s("public"), None], &[s("empty"), s("nothing here")]]),
            tables: rs(
                5,
                &[
                    &[s("public"), s("orders"), s("r"), None, s("101")],
                    &[s("public"), s("people"), s("r"), s("who"), s("100")],
                    &[s("public"), s("v_people"), s("v"), None, s("102")],
                    &[s("public"), s("weird"), s("?"), None, None],
                ],
            ),
            columns: rs(
                9,
                &[
                    &[
                        s("public"),
                        s("people"),
                        s("id"),
                        s("integer"),
                        s("true"),
                        None,
                        s("a"),
                        s(""),
                        None,
                    ],
                    &[
                        s("public"),
                        s("people"),
                        s("age"),
                        s("integer"),
                        s("false"),
                        s("0"),
                        s(""),
                        s(""),
                        s("years"),
                    ],
                    &[
                        s("public"),
                        s("people"),
                        s("dbl"),
                        s("integer"),
                        s("false"),
                        s("(age * 2)"),
                        s(""),
                        s("s"),
                        None,
                    ],
                    &[
                        s("public"),
                        s("ghost"),
                        s("x"),
                        s("text"),
                        s("false"),
                        None,
                        s(""),
                        s(""),
                        None,
                    ],
                ],
            ),
            indexes: rs(
                9,
                &[
                    &[
                        s("public"),
                        s("people"),
                        s("people_pkey"),
                        s("true"),
                        s("true"),
                        s("btree"),
                        None,
                        s("1"),
                        s("id"),
                    ],
                    &[
                        s("public"),
                        s("people"),
                        s("people_name_idx"),
                        s("false"),
                        s("false"),
                        s("btree"),
                        s("(age > 0)"),
                        s("1"),
                        s("lower(name)"),
                    ],
                    &[
                        s("public"),
                        s("people"),
                        s("people_name_idx"),
                        s("false"),
                        s("false"),
                        s("btree"),
                        s("(age > 0)"),
                        s("2"),
                        s("age"),
                    ],
                ],
            ),
            constraints: rs(
                11,
                &[
                    &[
                        s("public"),
                        s("orders"),
                        s("orders_person_fk"),
                        s("f"),
                        s(
                            "FOREIGN KEY (person_id, tenant) REFERENCES people(id, tenant) ON DELETE CASCADE",
                        ),
                        s("person_id\u{1f}tenant"),
                        s("public"),
                        s("people"),
                        s("id\u{1f}tenant"),
                        s("a"),
                        s("c"),
                    ],
                    &[
                        s("public"),
                        s("people"),
                        s("people_pkey"),
                        s("p"),
                        s("PRIMARY KEY (id)"),
                        s("id"),
                        None,
                        None,
                        None,
                        s("a"),
                        s("a"),
                    ],
                    &[
                        s("public"),
                        s("people"),
                        s("age_ok"),
                        s("c"),
                        s("CHECK (age >= 0)"),
                        s("age"),
                        None,
                        None,
                        None,
                        s("a"),
                        s("a"),
                    ],
                ],
            ),
            sequences: rs(5, &[&[s("public"), s("people_id_seq"), s("integer"), s("1"), s("1")]]),
            routines: rs(
                5,
                &[
                    &[s("public"), s("add"), s("f"), s("a integer, b integer"), s("integer")],
                    &[s("public"), s("cleanup"), s("p"), s(""), None],
                ],
            ),
        }
    }

    #[test]
    fn records_the_connected_database_and_default_schema() {
        let c = assemble(&sample()).unwrap();
        assert_eq!(
            (c.database.as_deref(), c.default_schema.as_deref()),
            (Some("app"), Some("public"))
        );
        // `current_schema()` is NULL when the search path names no existing schema.
        let mut raw = sample();
        raw.session = rs(2, &[&[Some("app"), None]]);
        assert_eq!(assemble(&raw).unwrap().default_schema, None);
    }

    #[test]
    fn assembles_relations_and_children() {
        let c = assemble(&sample()).unwrap();
        assert_eq!(c.schemas.len(), 2);
        assert_eq!(c.schema("empty").unwrap().comment.as_deref(), Some("nothing here"));

        let public = c.schema("public").unwrap();
        // Unknown relkind is skipped, not an error.
        let kinds: Vec<_> = public.tables.iter().map(|t| (t.name.as_str(), t.kind)).collect();
        assert_eq!(
            kinds,
            [
                ("orders", TableKind::Table),
                ("people", TableKind::Table),
                ("v_people", TableKind::View)
            ]
        );
        let people = public.table("people").unwrap();
        assert_eq!(people.comment.as_deref(), Some("who"));
        assert_eq!(people.state.as_deref(), Some("100"));
    }

    #[test]
    fn columns_carry_defaults_identity_and_generation() {
        let c = assemble(&sample()).unwrap();
        let people = c.table("public", "people").unwrap();
        let [id, age, dbl] = &people.columns[..] else { panic!("{:?}", people.columns) };
        assert!(!id.nullable);
        assert_eq!(id.identity, Some(Identity::Always));
        assert!(age.nullable);
        assert_eq!(age.default.as_deref(), Some("0"));
        assert_eq!(age.comment.as_deref(), Some("years"));
        assert!(dbl.generated);
        assert_eq!(dbl.identity, None);
    }

    #[test]
    fn index_rows_fold_into_one_index() {
        let c = assemble(&sample()).unwrap();
        let people = c.table("public", "people").unwrap();
        assert_eq!(people.indexes.len(), 2);
        assert!(people.indexes[0].primary && people.indexes[0].unique);
        let by_name = &people.indexes[1];
        assert_eq!(by_name.columns, ["lower(name)", "age"]);
        assert_eq!(by_name.predicate.as_deref(), Some("(age > 0)"));
    }

    #[test]
    fn constraints_resolve_keys_and_foreign_key_targets() {
        let c = assemble(&sample()).unwrap();
        assert_eq!(c.table("public", "people").unwrap().primary_key(), ["id"]);

        let orders = c.table("public", "orders").unwrap();
        let fk = orders.foreign_keys().next().expect("foreign key");
        assert_eq!(fk.columns, ["person_id", "tenant"]);
        let target = fk.reference.as_ref().unwrap();
        assert_eq!((target.schema.as_str(), target.table.as_str()), ("public", "people"));
        assert_eq!(target.columns, ["id", "tenant"]);
        assert_eq!((target.on_update, target.on_delete), (FkAction::NoAction, FkAction::Cascade));

        let people = c.table("public", "people").unwrap();
        let check = people.constraints.iter().find(|k| k.kind == ConstraintKind::Check).unwrap();
        assert!(check.reference.is_none());
    }

    #[test]
    fn sequences_and_routines_attach_to_their_schema() {
        let c = assemble(&sample()).unwrap();
        let public = c.schema("public").unwrap();
        assert_eq!(public.sequences[0].name, "people_id_seq");
        assert_eq!(public.routines[0].returns.as_deref(), Some("integer"));
        assert_eq!(public.routines[1].kind, RoutineKind::Procedure);
        assert_eq!(public.routines[1].returns, None);
    }

    #[test]
    fn a_wrong_column_count_is_reported_not_misread() {
        let mut raw = sample();
        raw.columns = rs(3, &[]);
        let err = assemble(&raw).unwrap_err();
        assert!(matches!(err, Error::Query(ref m) if m.contains("columns")), "{err}");
    }

    #[test]
    fn literals_escape_quotes_and_backslashes() {
        assert_eq!(literal("a'b"), "E'a''b'");
        assert_eq!(literal(r"a\b"), r"E'a\\b'");
    }

    #[test]
    fn schema_filter_honours_scope() {
        let all = schema_filter("s", &IntrospectScope::default());
        assert!(all.contains("information_schema") && !all.contains(" in (E"));
        let some = schema_filter("s", &IntrospectScope { schemas: Some(vec!["x".into()]) });
        assert!(some.ends_with("and s in (E'x')"), "{some}");
        let none = schema_filter("s", &IntrospectScope { schemas: Some(vec![]) });
        assert!(none.ends_with("and false"));
    }
}
