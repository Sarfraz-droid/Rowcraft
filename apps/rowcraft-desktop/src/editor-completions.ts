import { Cassandra, MSSQL, MariaSQL, MySQL, PLSQL, PostgreSQL, SQLite, StandardSQL, type SQLConfig, type SQLNamespace } from "@codemirror/lang-sql";
import type { Catalog } from "./api";

/** Use the active console's dialect and catalog without replacing its SQL draft. */
export function editorSqlConfig(dialect: string, catalog: Catalog | null, schema?: string): SQLConfig {
  const dialects = { mysql: MySQL, mariadb: MariaSQL, pg: PostgreSQL, pgbase: PostgreSQL, cockroach: PostgreSQL, redshift: PostgreSQL, greenplum: PostgreSQL, sqlite: SQLite, mssql: MSSQL, oracle: PLSQL, cassandra: Cassandra };
  const namespace: SQLNamespace = Object.fromEntries((catalog?.schemas ?? []).map(schema => [schema.name, {
    self: { label: schema.name, type: "namespace", detail: "schema" },
    children: Object.fromEntries(schema.tables.map(table => [table.name, {
      self: { label: table.name, type: "type", detail: table.kind === "View" || table.kind === "MaterializedView" ? "view" : "table", info: table.comment ?? undefined },
      children: table.columns.map(column => ({ label: column.name, type: "property", detail: column.native_type, info: column.comment ?? undefined })),
    }])),
  }]));
  return {
    dialect: dialects[dialect as keyof typeof dialects] ?? StandardSQL,
    schema: namespace,
    defaultSchema: schema ?? catalog?.default_schema ?? catalog?.schemas[0]?.name,
    upperCaseKeywords: true,
  };
}
