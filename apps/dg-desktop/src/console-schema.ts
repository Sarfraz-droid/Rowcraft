import type { Catalog } from "./api";

export function supportsConsoleSchema(dialect: string): boolean {
  return ["mysql", "mariadb", "pg", "pgbase", "cockroach", "redshift", "greenplum", "clickhouse", "mongo", "redis", "cassandra"].includes(dialect);
}

export function consoleSchema(chosen: string | undefined, catalog: Catalog | null): string {
  return chosen ?? catalog?.default_schema ?? catalog?.schemas[0]?.name ?? "";
}
