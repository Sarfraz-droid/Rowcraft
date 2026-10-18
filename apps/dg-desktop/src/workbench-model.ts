import type { Catalog, SourceSummary, TableInfo } from './api';

export type WorkspaceView = 'console' | 'dashboard' | 'schema-map' | 'databases' | 'history';
export interface QueryHistoryItem { id: string; source: string; statement: string; at: string; duration_ms: number; rows: number; error?: string }
export interface SourceViewProps {
  catalog: Catalog | null; source: SourceSummary | null; connected: boolean; busy: boolean;
  onOpenTable(schema: string, table: string): void; onRefresh(): void;
}
export type TableMetadata = TableInfo;
export interface CatalogTable { id: string; schema: string; table: TableMetadata }
export const tableId = (schema: string, name: string) => JSON.stringify([schema, name]);
export function catalogTables(catalog: Catalog | null): CatalogTable[] {
  return catalog?.schemas.flatMap(schema => schema.tables.map(table => ({ id: tableId(schema.name, table.name), schema: schema.name, table }))) ?? [];
}
export function filterTables(tables: CatalogTable[], schema: string, search: string) {
  const needle = search.trim().toLocaleLowerCase();
  return tables.filter(item => (!schema || item.schema === schema) && (!needle || `${item.schema}.${item.table.name}`.toLocaleLowerCase().includes(needle)));
}
export interface MapRelationship { id: string; from: string; to: string; columns: string[]; targetColumns: string[]; name: string; origin?: 'manual' | 'inferred'; evidence?: string }
export function relationships(tables: CatalogTable[]): MapRelationship[] {
  const ids = new Set(tables.map(item => item.id));
  return tables.flatMap(item => (item.table.constraints ?? []).flatMap((constraint, index) => {
    if (constraint.kind !== 'ForeignKey' || !constraint.reference) return [];
    const target = tableId(constraint.reference.schema || item.schema, constraint.reference.table);
    return ids.has(target) ? [{ id: `${item.id}:${index}`, from: item.id, to: target, columns: constraint.columns, targetColumns: constraint.reference.columns, name: constraint.name }] : [];
  }));
}
export function columnTypes(tables: CatalogTable[]) {
  const types = new Map<string, number>();
  tables.forEach(item => item.table.columns.forEach(column => { const type = column.native_type.replace(/\(.*/, '').trim().toUpperCase() || 'UNKNOWN'; types.set(type, (types.get(type) ?? 0) + 1); }));
  return [...types].map(([name, count]) => ({ name, count })).sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
}
