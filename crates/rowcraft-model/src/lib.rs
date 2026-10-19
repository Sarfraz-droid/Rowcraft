//! In-memory database object model, filled by introspection and persisted by `rowcraft-cache`.
//!
//! The model captures database objects per schema: relations of several
//! kinds, their columns (with defaults, identity and generation), indexes, constraints and the
//! schema-level sequences and routines.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    /// Database the connection was made to, when the database has the notion.
    #[serde(default)]
    pub database: Option<String>,
    /// Schema unqualified names resolve to (`current_schema()`); what `@` means in a schema pattern.
    #[serde(default)]
    pub default_schema: Option<String>,
    pub schemas: Vec<Schema>,
}

impl Catalog {
    pub fn schema(&self, name: &str) -> Option<&Schema> {
        self.schemas.iter().find(|s| s.name == name)
    }

    /// Looks a table up by `schema.table`.
    pub fn table(&self, schema: &str, table: &str) -> Option<&Table> {
        self.schema(schema)?.table(table)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schema {
    pub name: String,
    pub comment: Option<String>,
    pub tables: Vec<Table>,
    pub sequences: Vec<Sequence>,
    pub routines: Vec<Routine>,
}

impl Schema {
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }
}

/// What kind of relation a [`Table`] is. Views and friends share the table shape because they
/// have columns and appear side by side in the explorer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableKind {
    #[default]
    Table,
    PartitionedTable,
    View,
    MaterializedView,
    ForeignTable,
}

impl TableKind {
    /// Label used in explorer groups and CLI output.
    pub fn label(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::PartitionedTable => "partitioned table",
            Self::View => "view",
            Self::MaterializedView => "materialized view",
            Self::ForeignTable => "foreign table",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub kind: TableKind,
    pub comment: Option<String>,
    pub columns: Vec<Column>,
    pub indexes: Vec<Index>,
    pub constraints: Vec<Constraint>,
    /// Database-specific change marker (for PostgreSQL the row's `xmin`). Lets a later refresh
    /// skip objects that did not change. `None` when the database offers nothing usable.
    pub state: Option<String>,
}

impl Table {
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// Primary key columns in key order; empty when the table has none.
    pub fn primary_key(&self) -> &[String] {
        self.constraints
            .iter()
            .find(|c| c.kind == ConstraintKind::PrimaryKey)
            .map_or(&[], |c| c.columns.as_slice())
    }

    pub fn foreign_keys(&self) -> impl Iterator<Item = &Constraint> {
        self.constraints.iter().filter(|c| c.kind == ConstraintKind::ForeignKey)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    /// Native type as the database would print it, e.g. `character varying(32)`.
    pub native_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub identity: Option<Identity>,
    /// Computed column (`GENERATED ALWAYS AS (...) STORED`); the expression is in `default`.
    pub generated: bool,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Identity {
    Always,
    ByDefault,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub name: String,
    pub unique: bool,
    pub primary: bool,
    /// Access method (`btree`, `gin`, …) where the database has the notion.
    pub method: Option<String>,
    /// One entry per key column; an expression index keeps the expression text.
    pub columns: Vec<String>,
    /// Partial index predicate.
    pub predicate: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintKind {
    PrimaryKey,
    Unique,
    ForeignKey,
    Check,
    Exclusion,
}

/// Referential action of a foreign key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FkAction {
    #[default]
    NoAction,
    Restrict,
    Cascade,
    SetNull,
    SetDefault,
}

impl FkAction {
    pub fn sql(self) -> &'static str {
        match self {
            Self::NoAction => "NO ACTION",
            Self::Restrict => "RESTRICT",
            Self::Cascade => "CASCADE",
            Self::SetNull => "SET NULL",
            Self::SetDefault => "SET DEFAULT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraint {
    pub name: String,
    pub kind: ConstraintKind,
    pub columns: Vec<String>,
    /// The constraint as the database prints it (`CHECK (age > 0)`, `FOREIGN KEY (…) REFERENCES …`).
    pub definition: String,
    pub reference: Option<ForeignKeyTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignKeyTarget {
    pub schema: String,
    pub table: String,
    pub columns: Vec<String>,
    pub on_update: FkAction,
    pub on_delete: FkAction,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sequence {
    pub name: String,
    pub data_type: String,
    pub start: Option<String>,
    pub increment: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutineKind {
    #[default]
    Function,
    Procedure,
    Aggregate,
    Window,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Routine {
    pub name: String,
    pub kind: RoutineKind,
    /// Argument list as written in a signature, e.g. `a integer, b text`.
    pub arguments: String,
    /// Return type; `None` for procedures.
    pub returns: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(columns: &[&str]) -> Constraint {
        Constraint {
            name: "pk".into(),
            kind: ConstraintKind::PrimaryKey,
            columns: columns.iter().map(|c| (*c).to_owned()).collect(),
            definition: String::new(),
            reference: None,
        }
    }

    #[test]
    fn primary_key_comes_from_constraints() {
        let mut t = Table { name: "t".into(), ..Default::default() };
        assert!(t.primary_key().is_empty());
        t.constraints.push(pk(&["a", "b"]));
        assert_eq!(t.primary_key(), ["a", "b"]);
    }

    #[test]
    fn lookups_by_name() {
        let catalog = Catalog {
            schemas: vec![Schema {
                name: "public".into(),
                tables: vec![Table { name: "people".into(), ..Default::default() }],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(catalog.table("public", "people").is_some());
        assert!(catalog.table("public", "nope").is_none());
        assert!(catalog.table("other", "people").is_none());
    }

    #[test]
    fn catalog_round_trips_through_json() {
        // The cache stores catalogs serialized, so every field must survive.
        let mut t = Table { name: "t".into(), kind: TableKind::View, ..Default::default() };
        t.constraints.push(pk(&["a"]));
        let catalog = Catalog {
            database: Some("d".into()),
            default_schema: Some("s".into()),
            schemas: vec![Schema { name: "s".into(), tables: vec![t], ..Default::default() }],
        };
        let json = serde_json::to_string(&catalog).unwrap();
        assert_eq!(serde_json::from_str::<Catalog>(&json).unwrap(), catalog);
    }
}
