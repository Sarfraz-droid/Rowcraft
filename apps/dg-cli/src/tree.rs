//! Plain-text rendering of a [`Catalog`] for `dg introspect`.

use std::fmt::Write;

use dg_model::{
    Catalog, Column, Constraint, ConstraintKind, FkAction, Identity, Index, Routine, RoutineKind,
    Table,
};

pub fn render(catalog: &Catalog) -> String {
    let mut out = String::new();
    for schema in &catalog.schemas {
        let _ = write!(out, "schema {}", schema.name);
        comment(&mut out, schema.comment.as_deref());
        out.push('\n');
        for table in &schema.tables {
            render_table(&mut out, table);
        }
        for seq in &schema.sequences {
            let _ = writeln!(out, "  sequence {} ({})", seq.name, seq.data_type);
        }
        for routine in &schema.routines {
            let _ = writeln!(out, "  {}", routine_line(routine));
        }
    }
    out
}

fn comment(out: &mut String, text: Option<&str>) {
    if let Some(text) = text {
        let _ = write!(out, "  -- {}", text.replace('\n', " "));
    }
}

fn render_table(out: &mut String, table: &Table) {
    let _ = write!(out, "  {} {}", table.kind.label(), table.name);
    comment(out, table.comment.as_deref());
    out.push('\n');
    let width = table.columns.iter().map(|c| c.name.chars().count()).max().unwrap_or(0);
    for column in &table.columns {
        let _ = writeln!(out, "    {}", column_line(column, width, table.primary_key()));
    }
    for index in table.indexes.iter().filter(|i| !i.primary) {
        let _ = writeln!(out, "    {}", index_line(index));
    }
    // A primary key is shown on its columns, and a unique constraint is already listed as the
    // unique index that backs it.
    let backed_by_index = |c: &Constraint| {
        c.kind == ConstraintKind::Unique && table.indexes.iter().any(|i| i.name == c.name)
    };
    for constraint in table
        .constraints
        .iter()
        .filter(|c| c.kind != ConstraintKind::PrimaryKey && !backed_by_index(c))
    {
        let _ = writeln!(out, "    {}", constraint_line(constraint));
    }
}

fn column_line(column: &Column, width: usize, primary_key: &[String]) -> String {
    let mut line = format!("{:<width$}  {}", column.name, column.native_type);
    if !column.nullable {
        line.push_str("  not null");
    }
    match column.identity {
        Some(Identity::Always) => line.push_str("  identity always"),
        Some(Identity::ByDefault) => line.push_str("  identity by default"),
        None => {}
    }
    if let Some(expr) = &column.default {
        let word = if column.generated { "generated" } else { "default" };
        let _ = write!(line, "  {word} {expr}");
    }
    if primary_key.contains(&column.name) {
        line.push_str("  pk");
    }
    line
}

fn index_line(index: &Index) -> String {
    let unique = if index.unique { "unique " } else { "" };
    let mut line = format!("{unique}index {} ({})", index.name, index.columns.join(", "));
    if let Some(method) = index.method.as_deref().filter(|m| *m != "btree") {
        let _ = write!(line, " using {method}");
    }
    if let Some(predicate) = &index.predicate {
        let _ = write!(line, " where {predicate}");
    }
    line
}

fn constraint_line(c: &Constraint) -> String {
    match (&c.kind, &c.reference) {
        (ConstraintKind::ForeignKey, Some(target)) => {
            let mut line = format!(
                "foreign key {} ({}) -> {}.{} ({})",
                c.name,
                c.columns.join(", "),
                target.schema,
                target.table,
                target.columns.join(", ")
            );
            for (when, action) in [("update", target.on_update), ("delete", target.on_delete)] {
                if action != FkAction::NoAction {
                    let _ = write!(line, " on {when} {}", action.sql().to_lowercase());
                }
            }
            line
        }
        (ConstraintKind::Unique, _) => format!("unique {} ({})", c.name, c.columns.join(", ")),
        // Check and exclusion constraints are best shown as the database prints them.
        _ => format!("constraint {} {}", c.name, c.definition),
    }
}

fn routine_line(r: &Routine) -> String {
    let kind = match r.kind {
        RoutineKind::Function => "function",
        RoutineKind::Procedure => "procedure",
        RoutineKind::Aggregate => "aggregate",
        RoutineKind::Window => "window function",
    };
    match &r.returns {
        Some(returns) => format!("{kind} {}({}) -> {returns}", r.name, r.arguments),
        None => format!("{kind} {}({})", r.name, r.arguments),
    }
}

#[cfg(test)]
mod tests {
    use dg_model::{ForeignKeyTarget, Schema, Sequence, TableKind};

    use super::*;

    fn catalog() -> Catalog {
        let people = Table {
            name: "people".into(),
            comment: Some("who\nelse".into()),
            columns: vec![
                Column {
                    name: "id".into(),
                    native_type: "integer".into(),
                    identity: Some(Identity::Always),
                    ..Default::default()
                },
                Column {
                    name: "age".into(),
                    native_type: "integer".into(),
                    nullable: true,
                    default: Some("18".into()),
                    ..Default::default()
                },
            ],
            indexes: vec![
                Index {
                    name: "people_pkey".into(),
                    primary: true,
                    unique: true,
                    columns: vec!["id".into()],
                    ..Default::default()
                },
                Index {
                    name: "people_age".into(),
                    method: Some("hash".into()),
                    columns: vec!["age".into()],
                    predicate: Some("(age > 0)".into()),
                    ..Default::default()
                },
            ],
            constraints: vec![
                Constraint {
                    name: "people_pkey".into(),
                    kind: ConstraintKind::PrimaryKey,
                    columns: vec!["id".into()],
                    definition: "PRIMARY KEY (id)".into(),
                    reference: None,
                },
                Constraint {
                    name: "age_ok".into(),
                    kind: ConstraintKind::Check,
                    columns: vec!["age".into()],
                    definition: "CHECK (age >= 0)".into(),
                    reference: None,
                },
            ],
            ..Default::default()
        };
        let orders = Table {
            name: "orders".into(),
            kind: TableKind::View,
            constraints: vec![Constraint {
                name: "fk".into(),
                kind: ConstraintKind::ForeignKey,
                columns: vec!["person_id".into()],
                definition: String::new(),
                reference: Some(ForeignKeyTarget {
                    schema: "public".into(),
                    table: "people".into(),
                    columns: vec!["id".into()],
                    on_update: FkAction::NoAction,
                    on_delete: FkAction::Cascade,
                }),
            }],
            ..Default::default()
        };
        Catalog {
            schemas: vec![Schema {
                name: "public".into(),
                tables: vec![people, orders],
                sequences: vec![Sequence {
                    name: "s".into(),
                    data_type: "bigint".into(),
                    ..Default::default()
                }],
                routines: vec![
                    Routine {
                        name: "add".into(),
                        arguments: "a integer".into(),
                        returns: Some("integer".into()),
                        ..Default::default()
                    },
                    Routine {
                        name: "go".into(),
                        kind: RoutineKind::Procedure,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn renders_the_whole_tree() {
        let expected = "\
schema public
  table people  -- who else
    id   integer  not null  identity always  pk
    age  integer  default 18
    index people_age (age) using hash where (age > 0)
    constraint age_ok CHECK (age >= 0)
  view orders
    foreign key fk (person_id) -> public.people (id) on delete cascade
  sequence s (bigint)
  function add(a integer) -> integer
  procedure go()
";
        assert_eq!(render(&catalog()), expected);
    }

    #[test]
    fn a_unique_constraint_is_not_repeated_after_its_index() {
        let table = Table {
            name: "t".into(),
            indexes: vec![Index {
                name: "t_email_key".into(),
                unique: true,
                columns: vec!["email".into()],
                ..Default::default()
            }],
            constraints: vec![
                Constraint {
                    name: "t_email_key".into(),
                    kind: ConstraintKind::Unique,
                    columns: vec!["email".into()],
                    definition: String::new(),
                    reference: None,
                },
                // Same kind, but nothing backs it in the index list: it must still show.
                Constraint {
                    name: "t_other".into(),
                    kind: ConstraintKind::Unique,
                    columns: vec!["other".into()],
                    definition: String::new(),
                    reference: None,
                },
            ],
            ..Default::default()
        };
        let mut out = String::new();
        render_table(&mut out, &table);
        assert_eq!(out.matches("t_email_key").count(), 1, "{out}");
        assert!(out.contains("unique t_other (other)"), "{out}");
    }

    #[test]
    fn an_empty_catalog_renders_nothing() {
        assert_eq!(render(&Catalog::default()), "");
    }
}
