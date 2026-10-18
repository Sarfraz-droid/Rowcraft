import type { WorkspaceTab, WorkspaceTabKind } from "./workspace-tabs";
import type { QueryHistoryItem } from "./workbench-model";
import type { WorkspaceSettings } from "./workspaces";

export type WorkspaceSession = { tabs: WorkspaceTab[]; activeId: string | null; selected: string | null; history: QueryHistoryItem[] };
export const WORKSPACE_SESSIONS_KEY = "dglite.workspace-sessions.v1";
const kinds = new Set<WorkspaceTabKind>(["console", "table", "schema", "dashboard", "schema-map", "databases", "history"]);
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);

export function loadWorkspaceSessions(settings: WorkspaceSettings): Map<string, WorkspaceSession> {
  const sessions = new Map<string, WorkspaceSession>();
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(WORKSPACE_SESSIONS_KEY) ?? "null");
    if (!record(saved) || saved.version !== 1 || !Array.isArray(saved.sessions)) return sessions;
    for (const entry of saved.sessions) {
      if (!record(entry) || typeof entry.workspaceId !== "string" || !Array.isArray(entry.tabs)) continue;
      const workspace = settings.workspaces.find(item => item.id === entry.workspaceId);
      if (!workspace) continue;
      const tabs: WorkspaceTab[] = [];
      const ids = new Set<string>();
      for (const tab of entry.tabs) {
        if (!record(tab) || typeof tab.id !== "string" || !tab.id || ids.has(tab.id)
          || typeof tab.source !== "string" || !workspace.sources.includes(tab.source)
          || typeof tab.kind !== "string" || !kinds.has(tab.kind as WorkspaceTabKind)
          || typeof tab.title !== "string" || typeof tab.sql !== "string"
          || (tab.schema !== undefined && typeof tab.schema !== "string")
          || (tab.table !== undefined && typeof tab.table !== "string")) continue;
        ids.add(tab.id);
        tabs.push({
          id: tab.id, source: tab.source, kind: tab.kind as WorkspaceTabKind, title: tab.title,
          sql: tab.sql, preview: tab.preview === true && tab.kind !== "console", schema: tab.schema, table: tab.table, result: null,
          resultView: tab.resultView === "json" ? "json" : tab.resultView === "chart" ? "chart" : "table",
          split: typeof tab.split === "number" && Number.isFinite(tab.split) ? Math.min(.9, Math.max(.1, tab.split)) : .55,
        });
      }
      const activeId = typeof entry.activeId === "string" && ids.has(entry.activeId) ? entry.activeId : tabs[0]?.id ?? null;
      const selected = tabs.find(tab => tab.id === activeId)?.source
        ?? (typeof entry.selected === "string" && workspace.sources.includes(entry.selected) ? entry.selected : null);
      sessions.set(workspace.id, { tabs, activeId, selected, history: [] });
    }
  } catch { /* Invalid or unavailable storage must not prevent opening the app. */ }
  return sessions;
}

/** Save documents and drafts, excluding result data, errors, and live connections. */
export function saveWorkspaceSessions(settings: WorkspaceSettings, sessions: Map<string, WorkspaceSession>): void {
  localStorage.setItem(WORKSPACE_SESSIONS_KEY, JSON.stringify({
    version: 1,
    sessions: settings.workspaces.flatMap(workspace => {
      const session = sessions.get(workspace.id);
      return session ? [{
        workspaceId: workspace.id, activeId: session.activeId, selected: session.selected,
        tabs: session.tabs.filter(tab => workspace.sources.includes(tab.source)).map(tab => ({
          id: tab.id, source: tab.source, kind: tab.kind, title: tab.title,
          schema: tab.schema, table: tab.table, sql: tab.sql, preview: tab.preview, resultView: tab.resultView, split: tab.split,
        })),
      }] : [];
    }),
  }));
}
