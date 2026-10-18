// @vitest-environment happy-dom
import { beforeEach, expect, it } from "vitest";
import { createWorkspaceTab } from "./workspace-tabs";
import { loadWorkspaceSessions, saveWorkspaceSessions, WORKSPACE_SESSIONS_KEY } from "./workspace-sessions";
import type { WorkspaceSettings } from "./workspaces";

const settings: WorkspaceSettings = { version: 1, activeId: "one", workspaces: [{ id: "one", name: "One", sources: ["local"] }] };
beforeEach(() => localStorage.clear());

it("round trips document state without saving returned database rows, errors, or query history", () => {
  const tab = { ...createWorkspaceTab("local", "table", { schema: "main", table: "people" }), sql: "SELECT * FROM people", resultView: "chart" as const, split: .7,
    result: { columns: ["private"], rows: [["private result"]], summary: "1 row" }, error: "old error" };
  saveWorkspaceSessions(settings, new Map([["one", { tabs: [tab], activeId: tab.id, selected: "local", history: [{ id: "history", source: "local", statement: "history SQL", at: "now", rows: 1, duration_ms: 10 }] }]]));
  const stored = localStorage.getItem(WORKSPACE_SESSIONS_KEY)!;
  expect(stored).not.toContain("private result");
  expect(stored).not.toContain("old error");
  expect(stored).not.toContain("history SQL");
  const restored = loadWorkspaceSessions(settings).get("one")!;
  expect(restored.activeId).toBe(tab.id);
  expect(restored.tabs[0]).toMatchObject({ id: tab.id, kind: "table", schema: "main", table: "people", sql: tab.sql, split: .7, resultView: "chart", result: null });
  expect(restored.history).toEqual([]);
});

it("ignores invalid storage and tabs for deleted sources, and repairs a missing active tab", () => {
  localStorage.setItem(WORKSPACE_SESSIONS_KEY, "invalid JSON");
  expect(loadWorkspaceSessions(settings).size).toBe(0);
  const valid = createWorkspaceTab("local", "console");
  localStorage.setItem(WORKSPACE_SESSIONS_KEY, JSON.stringify({ version: 1, sessions: [{ workspaceId: "one", tabs: [null, { ...valid, source: "removed" }, valid, valid, { ...valid, id: "invalid", kind: "unknown" }], activeId: "missing", selected: "removed" }] }));
  const restored = loadWorkspaceSessions(settings).get("one")!;
  expect(restored.tabs).toHaveLength(1);
  expect(restored.activeId).toBe(valid.id);
  expect(restored.selected).toBe("local");
});
