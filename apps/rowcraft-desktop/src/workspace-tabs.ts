import type { QueryOutput } from './api';
import type { WorkspaceView } from './workbench-model';

export type WorkspaceTabKind = WorkspaceView | 'table' | 'schema';
export interface WorkspaceTab {
  id: string;
  source: string;
  kind: WorkspaceTabKind;
  title: string;
  preview?: boolean;
  schema?: string;
  table?: string;
  sql: string;
  result: QueryOutput | null;
  resultStatement?: string;
  resultSchema?: string;
  resultView: 'table' | 'chart' | 'json';
  split: number;
  error?: string;
}
const titles: Record<WorkspaceTabKind, string> = {
  console: 'SQL console', dashboard: 'Overview', 'schema-map': 'Schema map',
  databases: 'Databases', history: 'Query history', table: 'Table', schema: 'Schema browser',
};
export function createWorkspaceTab(source: string, kind: WorkspaceTabKind, options: { schema?: string; table?: string; title?: string } = {}): WorkspaceTab {
  return {
    id: crypto.randomUUID(), source, kind,
    title: options.title ?? options.table ?? (kind === 'schema' && options.schema ? options.schema : titles[kind]),
    schema: options.schema, table: options.table,
    sql: '', result: null, resultView: 'table', split: .55,
  };
}
/** Consoles are independent scratch documents; object tabs have a per-source identity. */
export function findExistingTab(tabs: WorkspaceTab[], source: string, kind: WorkspaceTabKind, schema?: string, table?: string): WorkspaceTab | undefined {
  if (kind === 'console') return undefined;
  return tabs.find(tab => tab.source === source && tab.kind === kind && (kind !== 'table' && kind !== 'schema' || tab.schema === schema) && (kind !== 'table' || tab.table === table));
}
export function closeWorkspaceTab(tabs: WorkspaceTab[], activeId: string | null, id: string): { tabs: WorkspaceTab[]; activeId: string | null } {
  const index = tabs.findIndex(tab => tab.id === id);
  if (index < 0) return { tabs, activeId };
  const remaining = tabs.filter(tab => tab.id !== id);
  return { tabs: remaining, activeId: activeId === id ? (remaining[index] ?? remaining[index - 1])?.id ?? null : activeId };
}

/** Browsing replaces the temporary document; scratch consoles always keep their own tab. */
export function placeWorkspaceTab(tabs: WorkspaceTab[], tab: WorkspaceTab, preview: boolean): WorkspaceTab[] {
  const next = { ...tab, preview: preview && tab.kind !== 'console' };
  const index = next.preview ? tabs.findIndex(item => item.preview) : -1;
  return index < 0 ? [...tabs, next] : tabs.map((item, at) => at === index ? next : item);
}
