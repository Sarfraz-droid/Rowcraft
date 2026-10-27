import { QueryHelp } from "./components/QueryHelp";
import { InlineSqlAssistant, type InlineTarget } from "./components/InlineSqlAssistant";
import { SettingsDialog, type SettingsSection } from "./components/SettingsDialog";
import { getAppScale, DEFAULT_EDITOR_PREFERENCES, EDITOR_FONT_SIZES, EDITOR_PREFERENCES_KEY, loadEditorPreferences } from "./preferences";
import { SqlAssistant } from "./components/SqlAssistant";
import { ErrorNotice } from "./components/ErrorNotice";
import { ExportToast } from "./components/ExportToast";
import { SelectField } from "./components/SelectField";
import { useCallback, useEffect, useRef, useState } from "react";
import { type AddedSource, type Catalog, type RowChanges, type SourceSummary, api, errorText } from "./api";
import { AddSourceDialog } from "./components/AddSourceDialog";
import { DatabaseBrowser } from "./components/DatabaseBrowser";
import { SourceDashboard } from "./components/SourceDashboard";
import { SchemaMap } from "./components/SchemaMap";
import { QueryHistory } from "./components/QueryHistory";
import { ResultDocuments } from "./components/ResultDocuments";
import { ResultChart } from "./components/ResultChart";
import { Editor, type EditorHandle } from "./components/Editor";
import { Icon } from "./components/Icon";
import { AppUpdate } from "./components/AppUpdate";
import { ResultGrid, type GridEditing } from "./components/ResultGrid";
import { editSupport, findTable, sourceTable } from "./result-edit";
import { Sidebar } from "./components/Sidebar";
import { Splitter } from "./components/Splitter";
import { WorkspaceTabs } from "./components/WorkspaceTabs";
import { TableWorkspace } from "./components/TableWorkspace";
import { databaseProfile } from "./database-profile";
import { consoleSchema, supportsConsoleSchema } from "./console-schema";
import { resultToCsv } from "./result-tools";
import { closeWorkspaceTab, placeWorkspaceTab, createWorkspaceTab, findExistingTab, type WorkspaceTab, type WorkspaceTabKind } from "./workspace-tabs";
import type { QueryHistoryItem, WorkspaceView } from "./workbench-model";

import { loadWorkspaces, reconcileWorkspaceSources, WORKSPACES_KEY, type WorkspaceSettings } from "./workspaces";
import { loadWorkspaceSessions, saveWorkspaceSessions, type WorkspaceSession } from "./workspace-sessions";

const IS_MAC = navigator.userAgent.includes("Mac");
const RUN_SHORTCUT = IS_MAC ? "⌘ ↵" : "Ctrl ↵";
type Activity = "idle" | "connecting" | "running" | "refreshing";
type Message = { kind: "none" | "info" | "error"; text: string };
type AddKind = "source" | "sqlite" | "driver";

export function App() {
  const [allSources, setSources] = useState<SourceSummary[]>([]);
  const [workspaceSettings, setWorkspaceSettings] = useState(loadWorkspaces);
  const workspace = workspaceSettings.workspaces.find((item) => item.id === workspaceSettings.activeId)!;
  const sources = allSources.filter((item) => workspace.sources.includes(item.name));
  const [restoredSessions] = useState(() => loadWorkspaceSessions(workspaceSettings));
  const sessions = useRef<Map<string, WorkspaceSession>>(restoredSessions);
  const restored = restoredSessions.get(workspaceSettings.activeId);
  const [selected, setSelected] = useState<string | null>(restored?.selected ?? null);
  const [connected, setConnected] = useState<string | null>(null);
  const [liveConnections, setLiveConnections] = useState<string[]>([]);
  const liveConnectionsRef = useRef(new Set<string>());
  const [tabs, setTabs] = useState<WorkspaceTab[]>(restored?.tabs ?? []);
  const [activeId, setActiveId] = useState<string | null>(restored?.activeId ?? null);
  const [catalogs, setCatalogs] = useState<Record<string, Catalog>>({});
  const [discovering, setDiscovering] = useState<string[]>([]);
  const [activity, setActivity] = useState<Activity>("idle");
  const [runningTab, setRunningTab] = useState<string | null>(null);
  const [message, setMessage] = useState<Message>({ kind: "none", text: "" });
  const [exportedPath, setExportedPath] = useState<string | null>(null);
  const [aiSettingsVersion, setAiSettingsVersion] = useState(0);
  const [settingsSection, setSettingsSection] = useState<SettingsSection | null>(null);
  const [editorPreferences, setEditorPreferences] = useState(loadEditorPreferences);
  const [assistantOpen, setAssistantOpen] = useState(true);
  const [inlineTarget, setInlineTarget] = useState<InlineTarget | null>(null);
  const [adding, setAdding] = useState<AddKind | null>(null);
  const [editingSource, setEditingSource] = useState<string | null>(null);
  const [history, setHistory] = useState<QueryHistoryItem[]>([]);
  const [sidebarHidden, setSidebarHidden] = useState(false);
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    try { const saved = localStorage.getItem("rowcraft.theme"); if (saved === "light" || saved === "dark") return saved; } catch { /* Optional storage. */ }
    return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  });
  const tabsRef = useRef<WorkspaceTab[]>(tabs);
  const activeRef = useRef<string | null>(activeId);
  const connectedRef = useRef<string | null>(null);
  const locked = useRef(false);
  const editors = useRef(new Map<string, { current: EditorHandle | null }>());
  const consoleNumbers = useRef(new Map<string, number>());
  const paneElements = useRef(new Map<string, { current: HTMLDivElement | null }>());
  const persistSession = useRef<() => boolean>(() => true);
  persistSession.current = () => {
    const drafts = tabsRef.current.map(tab => tab.kind === "console" ? { ...tab, sql: editors.current.get(tab.id)?.current?.text() ?? tab.sql } : tab);
    sessions.current.set(workspace.id, { tabs: drafts, activeId: activeRef.current, selected, history });
    try { saveWorkspaceSessions(workspaceSettings, sessions.current); return true; }
    catch { setMessage({ kind: "error", text: "Open tabs and SQL drafts could not be saved. Free up local storage and try again before closing the app." }); return false; }
  };
  const busy = activity !== "idle";
  const active = tabs.find((tab) => tab.id === activeId) ?? null;
  const source = sources.find((item) => item.name === selected) ?? null;
  const catalog = selected ? catalogs[selected] ?? null : null;
  const sidebarView: WorkspaceView = active?.kind === "table" ? "console" : active?.kind === "schema" ? "databases" : active?.kind ?? "dashboard";
  const updateTabs = (change: (before: WorkspaceTab[]) => WorkspaceTab[]) => {
    tabsRef.current = change(tabsRef.current); setTabs(tabsRef.current);
  };
  const patchTab = (id: string, patch: Partial<WorkspaceTab>) => updateTabs((before) => before.map((tab) => tab.id === id ? { ...tab, ...patch } : tab));
  const editorHandle = (id: string) => {
    let handle = editors.current.get(id);
    if (!handle) { handle = { current: null }; editors.current.set(id, handle); }
    return handle;
  };
  const paneHandle = (id: string) => {
    let handle = paneElements.current.get(id);
    if (!handle) { handle = { current: null }; paneElements.current.set(id, handle); }
    return handle;
  };
  const activate = (tab: WorkspaceTab | null) => {
    activeRef.current = tab?.id ?? null; setActiveId(tab?.id ?? null);
    if (tab) setSelected(tab.source);
  };

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.classList.toggle("dark", theme === "dark");
    try { localStorage.setItem("rowcraft.theme", theme); } catch { /* Optional storage. */ }
  }, [theme]);
  useEffect(() => {
    const scale = editorPreferences.fontSize / DEFAULT_EDITOR_PREFERENCES.fontSize;
    const root = document.documentElement;
    root.style.setProperty("zoom", String(scale));
    root.style.setProperty("--app-viewport-height", `calc(100dvh / ${scale})`);
    root.style.setProperty("--app-viewport-width", `calc(100vw / ${scale})`);
    return () => {
      root.style.removeProperty("zoom");
      root.style.removeProperty("--app-viewport-height");
      root.style.removeProperty("--app-viewport-width");
    };
  }, [editorPreferences.fontSize]);
  useEffect(() => {
    api.bootstrap().then((boot) => { setSources(boot.sources); setWorkspaceSettings((before) => reconcileWorkspaceSources(before, boot.sources.map((item) => item.name))); if (boot.startup_error) setMessage({ kind: "error", text: boot.startup_error }); })
      .catch((e: unknown) => setMessage({ kind: "error", text: errorText(e) }));
  }, []);

  useEffect(() => {
    try { localStorage.setItem(WORKSPACES_KEY, JSON.stringify(workspaceSettings)); }
    catch { setMessage({ kind: "error", text: "Workspace changes could not be saved. Keep this window open and enable local storage to save them." }); }
  }, [workspaceSettings]);

  useEffect(() => { persistSession.current(); }, [tabs, activeId, selected, workspaceSettings]);
  useEffect(() => {
    const save = () => persistSession.current();
    const hidden = () => { if (document.visibilityState === "hidden") save(); };
    window.addEventListener("pagehide", save);
    window.addEventListener("beforeunload", save);
    document.addEventListener("visibilitychange", hidden);
    return () => {
      window.removeEventListener("pagehide", save);
      window.removeEventListener("beforeunload", save);
      document.removeEventListener("visibilitychange", hidden);
    };
  }, []);

  const switchWorkspace = (id: string, settings: WorkspaceSettings = workspaceSettings) => {
    if (locked.current || id === workspace.id) return;
    const drafts = tabsRef.current.map((tab) => tab.kind === "console" ? { ...tab, sql: editors.current.get(tab.id)?.current?.text() ?? tab.sql } : tab);
    sessions.current.set(workspace.id, { tabs: drafts, activeId: activeRef.current, selected, history });
    const next = sessions.current.get(id);
    updateTabs(() => next?.tabs ?? []);
    activeRef.current = next?.activeId ?? null; setActiveId(activeRef.current);
    setSelected(next?.selected ?? null); setHistory(next?.history ?? []);
    setMessage({ kind: "none", text: "" });
    setWorkspaceSettings({ ...settings, activeId: id });
  };
  const moveSource = (name: string, destinationId: string) => {
    const destination = workspaceSettings.workspaces.find(item => item.id === destinationId);
    if (locked.current || !destination || destinationId === workspace.id || !workspace.sources.includes(name)) return;
    const moved = tabsRef.current.filter(tab => tab.source === name).map(tab => ({ ...tab, preview: false, sql: editors.current.get(tab.id)?.current?.text() ?? tab.sql }));
    const target = sessions.current.get(destinationId) ?? { tabs: [], activeId: null, selected: null, history: [] };
    const targetTabs = [...target.tabs, ...moved.filter(tab => !target.tabs.some(existing => existing.id === tab.id))];
    sessions.current.set(destinationId, {
      tabs: targetTabs, activeId: target.activeId ?? moved.find(tab => tab.id === activeRef.current)?.id ?? moved[0]?.id ?? null, selected: target.selected ?? name,
      history: [...history.filter(item => item.source === name), ...target.history].slice(0, 200),
    });
    const remaining = tabsRef.current.filter(tab => tab.source !== name);
    const next = remaining.find(tab => tab.id === activeRef.current) ?? remaining[0] ?? null;
    updateTabs(() => remaining);
    activate(next);
    if (!next && selected === name) setSelected(null);
    setHistory(before => before.filter(item => item.source !== name));
    for (const tab of moved) { editors.current.delete(tab.id); paneElements.current.delete(tab.id); }
    setInlineTarget(null);
    setWorkspaceSettings(before => ({ ...before, workspaces: before.workspaces.map(item => ({ ...item,
      sources: item.id === workspace.id ? item.sources.filter(source => source !== name)
        : item.id === destinationId ? [...new Set([...item.sources, name])] : item.sources,
    })) }));
    setMessage({ kind: "info", text: `Moved ${name} to ${destination.name}.` });
  };

  const createWorkspace = (name: string) => {
    if (locked.current || !name.trim() || workspaceSettings.workspaces.some((item) => item.name.toLowerCase() === name.trim().toLowerCase())) return;
    const next = { id: crypto.randomUUID(), name: name.trim(), sources: [] };
    switchWorkspace(next.id, { ...workspaceSettings, workspaces: [...workspaceSettings.workspaces, next] });
  };

  const renameWorkspace = (name: string) => {
    const trimmed = name.trim();
    if (locked.current || !trimmed || workspaceSettings.workspaces.some((item) => item.id !== workspace.id && item.name.toLowerCase() === trimmed.toLowerCase())) return;
    setWorkspaceSettings((before) => ({ ...before, workspaces: before.workspaces.map((item) => item.id === workspace.id ? { ...item, name: trimmed } : item) }));
  };

  const operation = async (kind: Activity, action: () => Promise<void>) => {
    if (locked.current) return;
    locked.current = true; setActivity(kind);
    try { await action(); }
    catch (e) { setMessage({ kind: "error", text: errorText(e) }); }
    finally { locked.current = false; setActivity("idle"); setRunningTab(null); }
  };
  const invalidateConnection = (name: string) => {
    liveConnectionsRef.current.delete(name); setLiveConnections([...liveConnectionsRef.current]);
    if (connectedRef.current === name) { connectedRef.current = null; setConnected(null); }
  };
  const connectionActivity = (name: string): Activity => liveConnectionsRef.current.has(name) ? "idle" : "connecting";
  const ensureConnection = async (name: string, reconnect = false) => {
    if (!reconnect && connectedRef.current === name) return;
    const retained = !reconnect && liveConnectionsRef.current.has(name);
    if (reconnect) invalidateConnection(name);
    if (!retained) setMessage({ kind: "info", text: `Connecting to ${name}…` });
    try {
      const opened = reconnect ? await api.connect(name, true) : await api.connect(name);
      connectedRef.current = opened; setConnected(opened);
      liveConnectionsRef.current.add(opened); setLiveConnections([...liveConnectionsRef.current]);
      if (retained) return;
      setMessage({ kind: "info", text: `Connected to ${opened}. Reading database objects…` });
      // Not awaited: on a large server the catalog takes a while, and the workbench stays usable
      // meanwhile (queries simply wait their turn on the connection).
      setDiscovering((before) => [...before, name]);
      api.introspect()
        .then((discovered) => {
          setCatalogs((before) => ({ ...before, [name]: discovered }));
          if (connectedRef.current === name) setMessage({ kind: "info", text: `Connected to ${opened}` });
        })
        .catch((e) => setMessage({ kind: "error", text: `Connected. Schema discovery failed: ${errorText(e)}` }))
        .finally(() => setDiscovering((before) => before.filter((item) => item !== name)));
    } catch (error) { connectedRef.current = null; setConnected(null); invalidateConnection(name); throw error; }
  };
  const makeTab = (name: string, kind: WorkspaceTabKind, options: { schema?: string; table?: string; sql?: string } = {}, newConsole = false, preview = kind !== "console") => {
    const existing = kind === "console" && !newConsole
      ? tabsRef.current.find((tab) => tab.source === name && tab.kind === "console")
      : findExistingTab(tabsRef.current, name, kind, options.schema, options.table);
    if (existing) { if (!preview && existing.preview) patchTab(existing.id, { preview: false }); activate(existing); return existing; }
    const usedNumbers = tabsRef.current.filter((tab) => tab.source === name && tab.kind === "console").map((tab) => Number(/^Console (\d+)$/.exec(tab.title)?.[1]) || 0);
    const number = Math.max(consoleNumbers.current.get(name) ?? 0, ...usedNumbers) + 1;
    if (kind === "console") consoleNumbers.current.set(name, number);
    const tab = createWorkspaceTab(name, kind, { ...options, title: kind === "console" ? `Console ${number}` : undefined });
    if (databaseProfile(sources.find(source => source.name === name)?.dialect).documentView) tab.resultView = "json";
    tab.sql = options.sql ?? (kind === "console" ? databaseProfile(sources.find(source => source.name === name)?.dialect).starter : "");
    const previousPreview = preview ? tabsRef.current.find(item => item.preview) : undefined;
    if (previousPreview) { editors.current.delete(previousPreview.id); paneElements.current.delete(previousPreview.id); }
    const next = placeWorkspaceTab(tabsRef.current, tab, preview);
    updateTabs(() => next); activate(tab); return tab;
  };
  const openView = (name: string, kind: WorkspaceTabKind, options: { schema?: string; table?: string; sql?: string } = {}, newConsole = false, preview = kind !== "console") => operation(connectionActivity(name), async () => {
    makeTab(name, kind, options, newConsole, preview); await ensureConnection(name);
  });
  const selectTab = (id: string) => operation(connectionActivity(tabsRef.current.find(tab => tab.id === id)?.source ?? ""), async () => {
    const tab = tabsRef.current.find((item) => item.id === id); if (!tab) return;
    activate(tab); await ensureConnection(tab.source);
  });
  const closeTab = (id: string) => operation("idle", async () => {
    const next = closeWorkspaceTab(tabsRef.current, activeRef.current, id);
    updateTabs(() => next.tabs); editors.current.delete(id); paneElements.current.delete(id);
    const chosen = next.tabs.find((tab) => tab.id === next.activeId) ?? null;
    activate(chosen);
    try { if (chosen) await ensureConnection(chosen.source); }
    finally {
      requestAnimationFrame(() => {
        const target = chosen ? document.getElementById(`document-tab-${chosen.id}`) : document.querySelector<HTMLElement>('.workspace-add-tab');
        target?.focus();
      });
    }
  });
  const performQuery = async (tab: WorkspaceTab, statement: string, showAll = false) => {
    setRunningTab(tab.id); setActivity("running");
    setMessage({ kind: "info", text: `Running ${tab.title}…` });
    const started = performance.now();
    const entry = { id: crypto.randomUUID(), source: tab.source, statement, at: new Date().toISOString() };
    try {
      const dialect = sources.find((item) => item.name === tab.source)?.dialect ?? "";
      const schema = (tab.kind === "console" || tab.kind === "table") && supportsConsoleSchema(dialect) ? consoleSchema(tab.schema, catalogs[tab.source] ?? null) : "";
      const output = showAll ? await api.runQuery(statement, schema || undefined, true) : schema ? await api.runQuery(statement, schema) : await api.runQuery(statement);
      patchTab(tab.id, { result: output, resultStatement: statement, resultSchema: schema, error: undefined });
      setHistory((before) => [{ ...entry, duration_ms: Math.round(performance.now() - started), rows: output.rows.length }, ...before].slice(0, 200));
      setMessage({ kind: "info", text: output.summary });
    } catch (e) {
      const error = errorText(e); patchTab(tab.id, { error });
      setHistory((before) => [{ ...entry, duration_ms: Math.round(performance.now() - started), rows: 0, error }, ...before].slice(0, 200));
      throw e;
    }
  };
  const showAllRows = (id: string) => operation("running", async () => {
    const tab = tabsRef.current.find(item => item.id === id);
    if (!tab?.result?.row_limit || !tab.resultStatement) return;
    await ensureConnection(tab.source);
    await performQuery({ ...tab, schema: tab.resultSchema }, tab.resultStatement, true);
  });
  /** Writes grid edits to `schema.table`, then reruns the tab's query so the grid shows the table as saved. */
  const saveRows = async (id: string, schema: string, table: string, changes: RowChanges) => {
    let saved = false;
    await operation("running", async () => {
      const tab = tabsRef.current.find((item) => item.id === id);
      const result = tab?.result;
      if (!tab || !result) return;
      await ensureConnection(tab.source);
      const output = await api.applyRowChanges(schema, table, result.columns, changes);
      saved = true;
      const statement = tab.resultStatement ?? tab.sql;
      if (statement) await performQuery(tab, statement);
      setMessage({ kind: "info", text: `Saved ${table}: ${output.summary}` });
    });
    return saved;
  };
  /** Whether a tab's result can be edited in the grid: it must come from one table whose rows have a key. */
  const resultEditing = (tab: WorkspaceTab): { editing?: GridEditing; readOnly?: string } => {
    const result = tab.result;
    const dialect = sources.find((item) => item.name === tab.source)?.dialect;
    if (!result?.columns.length || !databaseProfile(dialect).relationships) return {};
    const origin = tab.kind === "table" ? { schema: tab.schema, table: tab.table ?? tab.title, columns: "*" as const } : sourceTable(tab.resultStatement ?? "");
    if (!origin) return { readOnly: "Only the result of a plain select from a single table can be edited." };
    const catalog = catalogs[tab.source] ?? null;
    const found = findTable(catalog, origin, tab.resultSchema || tab.schema);
    if (!found) return { readOnly: catalog ? `${origin.table} is not among the loaded database objects; refresh them to edit its rows.` : "Database objects are still loading." };
    if (found.table.kind !== "Table" && found.table.kind !== "PartitionedTable") return { readOnly: `${found.table.name} is a view, so its rows are read-only here.` };
    const disabled = busy ? "Wait for the current operation to finish." : !liveConnections.includes(tab.source) ? `Connect to ${tab.source} to edit rows.` : undefined;
    return { editing: { ...editSupport(result.columns, found.table, origin.columns), disabled, onSave: (changes) => saveRows(tab.id, found.schema, found.table.name, changes) } };
  };
  const runTab = (id: string) => operation("running", async () => {
    const tab = tabsRef.current.find((item) => item.id === id); if (!tab) return;
    try {
      let statement = tab.kind === "console" ? editors.current.get(id)?.current?.statement() : tab.sql;
      if (!statement?.trim() && tab.kind !== "table") { setMessage({ kind: "info", text: "Write a SQL statement to run." }); return; }
      await ensureConnection(tab.source);
      if (tab.kind === "table" && !statement?.trim()) {
        statement = await api.previewStatement(tab.schema ?? "", tab.table ?? tab.title);
        patchTab(tab.id, { sql: statement });
      }
      if (statement) await performQuery(tab, statement);
    } catch (e) { patchTab(tab.id, { error: errorText(e) }); throw e; }
  });
  const openTable = (name: string, schema: string, tableName: string) => operation("running", async () => {
    const existing = findExistingTab(tabsRef.current, name, "table", schema, tableName);
    const tab = makeTab(name, "table", { schema, table: tableName });
    try {
      await ensureConnection(name);
      if (existing?.result) return;
      setRunningTab(tab.id);
      const statement = await api.previewStatement(schema, tableName);
      patchTab(tab.id, { sql: statement });
      await performQuery(tab, statement);
    } catch (e) { patchTab(tab.id, { error: errorText(e) }); throw e; }
  });
  const refresh = (name: string) => operation("refreshing", async () => {
    await ensureConnection(name);
    const discovered = await api.introspect(); setCatalogs((before) => ({ ...before, [name]: discovered }));
    setMessage({ kind: "info", text: "Database objects refreshed." });
  });
  const removeSource = (name: string) => operation("connecting", async () => {
    const remaining = await api.removeSource(name); setSources(remaining);
    setWorkspaceSettings((before) => reconcileWorkspaceSources(before, remaining.map((item) => item.name)));
    updateTabs((before) => before.filter((tab) => tab.source !== name));
    setCatalogs((before) => { const next = { ...before }; delete next[name]; return next; });
    setHistory((before) => before.filter((entry) => entry.source !== name));
    invalidateConnection(name);
    if (selected === name) {
      const chosen = tabsRef.current[0] ?? null; activate(chosen);
      if (chosen) await ensureConnection(chosen.source); else setSelected(null);
    }
    setMessage({ kind: "info", text: `Removed ${name}.` });
  });
  // The Tauri webview ignores `<a download>`, so the backend writes the file into Downloads.
  const exportTab = async (tab: WorkspaceTab) => {
    if (!tab.result) return;
    try { setExportedPath(await api.saveExport(`${tab.title}-results.csv`, resultToCsv(tab.result))); }
    catch (e) { setMessage({ kind: "error", text: `Export failed: ${errorText(e)}` }); }
  };
  const closeExportToast = useCallback(() => setExportedPath(null), []);
  const openInlineForTab = (tab: WorkspaceTab) => {
      const editor = editorHandle(tab.id).current;
      if (!editor) return;
      const fullSql = editor.text();
      const selection = editor.selection?.() ?? { from: 0, to: fullSql.length, text: fullSql };
      const coordinates = editor.cursorCoordinates?.();
      const pane = paneHandle(tab.id).current?.querySelector(".editor-pane")?.getBoundingClientRect();
      const scale = getAppScale();
      setInlineTarget({ tabId: tab.id, from: selection.from, to: selection.to, fullSql, selectedSql: selection.text,
        left: Math.max(8, Math.min(pane && coordinates ? (coordinates.left - pane.left) / scale : 16, (pane ? pane.width / scale : 500) - 456)),
        top: Math.max(8, Math.min(pane && coordinates ? (coordinates.bottom - pane.top) / scale + 8 : 16, (pane ? pane.height / scale : 400) - 360)),
      });
  };
  const keyboardAction = useRef<(event: KeyboardEvent) => void>(() => {});
  keyboardAction.current = (event) => {
    if (!(event.metaKey || event.ctrlKey)) return;
    if (!event.altKey && ["+", "=", "-", "0"].includes(event.key)) {
      event.preventDefault(); event.stopPropagation();
      const index = EDITOR_FONT_SIZES.indexOf(editorPreferences.fontSize as typeof EDITOR_FONT_SIZES[number]);
      const fontSize = event.key === "0" ? DEFAULT_EDITOR_PREFERENCES.fontSize
        : EDITOR_FONT_SIZES[Math.max(0, Math.min(EDITOR_FONT_SIZES.length - 1, index + (event.key === "-" ? -1 : 1)))] ?? DEFAULT_EDITOR_PREFERENCES.fontSize;
      const next = { ...editorPreferences, fontSize };
      setEditorPreferences(next);
      try { localStorage.setItem(EDITOR_PREFERENCES_KEY, JSON.stringify(next)); }
      catch { setMessage({ kind: "error", text: "Font size changed, but could not be saved on this device." }); }
      return;
    }
    if (adding || editingSource || settingsSection) return;
    // Keep the previous Option shortcut working as well as Command/Ctrl K.
    const outsideEditor = event.target instanceof HTMLElement && !!event.target.closest(".sidebar, .ai-assistant-sidebar");
    if (!outsideEditor && (event.code === "KeyK" || event.key.toLowerCase() === "k") && active?.kind === "console") {
      event.preventDefault(); event.stopPropagation();
      if (!inlineTarget) openInlineForTab(active);
      return;
    }
    if (inlineTarget || (event.target instanceof HTMLElement && event.target.closest(".ai-assistant-sidebar"))) return;
    if (event.key.toLowerCase() === "k") { event.preventDefault(); setSidebarHidden(false); requestAnimationFrame(() => {
      const toggle = document.getElementById("connections-toggle");
      if (toggle?.getAttribute("aria-expanded") === "false") toggle.click();
      requestAnimationFrame(() => document.getElementById("source-search")?.focus());
    }); }
    if (event.key === "Enter" && active?.kind === "console") { event.preventDefault(); event.stopPropagation(); void runTab(active.id); }
    if (event.key.toLowerCase() === "t" && selected) { event.preventDefault(); void openView(selected, "console", {}, true); }
    if (event.key.toLowerCase() === "w" && active) { event.preventDefault(); void closeTab(active.id); }
  };
  useEffect(() => {
    const listener = (event: KeyboardEvent) => keyboardAction.current(event);
    window.addEventListener("keydown", listener, true); return () => window.removeEventListener("keydown", listener, true);
  }, []);
  const added = useCallback((done: AddedSource) => {
    setSources(done.sources);
    setWorkspaceSettings((before) => ({ ...before, workspaces: before.workspaces.map((item) => item.id === workspace.id ? { ...item, sources: [...new Set([...item.sources, done.name])] } : item) }));
    setAdding(null); setMessage({ kind: "info", text: `Added ${done.name}. Select it to connect.` });
  }, [workspace.id]);
  const updatedSource = (done: AddedSource) => {
    const previous = editingSource;
    if (!previous) return;
    const rename = (name: string) => name === previous ? done.name : name;
    const update = (tab: WorkspaceTab): WorkspaceTab => tab.source === previous ? { ...tab, source: done.name, result: null, error: undefined } : tab;
    setSources(done.sources);
    setWorkspaceSettings(before => ({ ...before, workspaces: before.workspaces.map(item => ({ ...item, sources: item.sources.map(rename) })) }));
    for (const [id, session] of sessions.current) sessions.current.set(id, {
      ...session, tabs: session.tabs.map(update), selected: session.selected ? rename(session.selected) : null,
      history: session.history.map(entry => ({ ...entry, source: rename(entry.source) })),
    });
    updateTabs(before => before.map(update));
    if (selected === previous) setSelected(done.name);
    setHistory(before => before.map(entry => ({ ...entry, source: rename(entry.source) })));
    setCatalogs(before => { const next = { ...before }; delete next[previous]; delete next[done.name]; return next; });
    invalidateConnection(previous);
    if (done.name !== previous) invalidateConnection(done.name);
    if (inlineTarget && tabsRef.current.some(tab => tab.id === inlineTarget.tabId && tab.source === done.name)) setInlineTarget(null);
    setEditingSource(null);
    setMessage({ kind: "info", text: `Updated ${done.name}. Reconnect to use the saved connection settings.` });
  };
  const addTab = (kind: WorkspaceView | "schema") => {
    if (!selected) { setAdding("source"); return; }
    void openView(selected, kind, kind === "schema" ? { schema: catalog?.default_schema ?? catalog?.schemas[0]?.name } : {}, true, false);
  };

  return <div className="app slim-workbench">
    <div className="workspace">
      <Sidebar onKeepTab={id => patchTab(id, { preview: false })} onMoveSource={moveSource} onSaveAllEditors={() => {
        if (persistSession.current()) setMessage({ kind: "info", text: "All open editors and SQL drafts saved locally." });
      }} onCloseAllEditors={() => {
        if (busy) return;
        updateTabs(() => []); editors.current.clear(); paneElements.current.clear(); activate(null);
        requestAnimationFrame(() => document.querySelector<HTMLButtonElement>('.workspace-add-tab')?.focus());
      }} tabs={tabs} activeTabId={activeId} onSelectTab={id => void selectTab(id)} onCloseTab={id => void closeTab(id)} key={workspace.id} workspaces={workspaceSettings.workspaces} workspaceId={workspace.id} onSwitchWorkspace={switchWorkspace} onCreateWorkspace={createWorkspace} onRenameWorkspace={renameWorkspace} sources={sources} selected={selected} connected={connected} connectedSources={liveConnections} view={sidebarView} busy={busy} onNavigate={(name, kind) => void openView(name, kind)} onAdd={(kind = "source") => setAdding(kind)} onEdit={name => setEditingSource(name)} onRemove={(name) => void removeSource(name)} catalog={catalog} discovering={!!selected && discovering.includes(selected)} onOpenTable={(schema, table) => selected && void openTable(selected, schema, table)} onOpenSchema={(schema) => selected && void openView(selected, "schema", { schema })} onRefresh={() => selected && void refresh(selected)} onRefreshSource={(name) => void refresh(name)} onNewConsole={(name) => void openView(name, "console", {}, true)} onReconnect={(name) => void operation("connecting", async () => { makeTab(name, "dashboard"); await ensureConnection(name, true); })} hidden={sidebarHidden} />
      <main className="main">
        <div className="workspace-topbar"><div className="workspace-breadcrumb"><button type="button" className="icon-button" aria-label={sidebarHidden ? "Show sidebar" : "Hide sidebar"} title={sidebarHidden ? "Show sidebar" : "Hide sidebar"} aria-expanded={!sidebarHidden} aria-controls="workspace-sidebar" onClick={() => setSidebarHidden(hidden => !hidden)}><Icon name={sidebarHidden ? "sidebarExpand" : "sidebarCollapse"} size={17} /></button><span className="breadcrumb-source tw:text-foreground tw:whitespace-nowrap tw:overflow-hidden tw:text-ellipsis">{source?.name ?? workspace.name}</span>{active && <><Icon name="chevronRight" size={12} /><span>{active.kind === "table" && active.schema ? `${active.schema}.` : ""}{active.title}</span></>}</div><div className="workspace-controls">{source && <>{!liveConnections.includes(selected ?? "") && <button className="btn" disabled={busy} onClick={() => void operation("connecting", () => ensureConnection(source.name))}>Connect</button>}<span className="session-state"><span className={`connection-dot${liveConnections.includes(selected ?? "") ? " online" : ""}`} />{activity === "connecting" ? "Connecting…" : liveConnections.includes(selected ?? "") ? source.dialect.toUpperCase() : "Disconnected"}</span><button className="icon-button" disabled={busy} onClick={() => void refresh(source.name)} aria-label="Refresh schema" title="Refresh schema"><Icon name="refresh" className={activity === "refreshing" ? "rotating" : ""} /></button></>}<button type="button" className="icon-button" aria-label="Settings" title="Settings" onClick={() => setSettingsSection("general")}><Icon name="settings" /></button><button className="icon-button" aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} theme`} onClick={() => setTheme(theme === "dark" ? "light" : "dark")}><Icon name={theme === "dark" ? "sun" : "moon"} /></button></div></div>
        <WorkspaceTabs onKeep={id => patchTab(id, { preview: false })} dialect={source?.dialect} tabs={tabs} activeId={activeId} busy={busy} onSelect={(id) => void selectTab(id)} onClose={(id) => void closeTab(id)} onAdd={addTab} />
        {!tabs.length && <div className="welcome-workspace"><img src={`/brand/svg/logo-horizontal-${theme === "dark" ? "white" : "dark"}.svg`} alt="Rowcraft" width={158} height={25} className="tw:block tw:mb-[24px] tw:max-w-full tw:h-auto" /><h1>{source ? "Open a workspace tab" : "Open a data source"}</h1><p>{source ? "Open a table from the sidebar or create a console to write SQL." : "Connect a database to explore tables and write SQL."}</p><div className="row">{source ? <><button className="btn primary" onClick={() => addTab("console")}><Icon name="plus" /> New SQL console</button><button className="btn" onClick={() => addTab("databases")}>Browse databases</button></> : <button className="btn primary" onClick={() => setAdding("source")}><Icon name="plus" /> Add data source</button>}</div><div className="welcome-shortcuts"><span><kbd>{IS_MAC ? "⌘" : "Ctrl"} T</kbd> New console</span><span><kbd>{RUN_SHORTCUT}</kbd> Run query</span></div></div>}
        {tabs.map((tab) => {
          const tabSource = sources.find((item) => item.name === tab.source) ?? null;
          const tabCatalog = catalogs[tab.source] ?? null;
          const profile = databaseProfile(tabSource?.dialect);
          const props = { source: tabSource, catalog: tabCatalog, connected: liveConnections.includes(tab.source), busy: busy || discovering.includes(tab.source), onOpenTable: (schema: string, table: string) => void openTable(tab.source, schema, table), onRefresh: () => void refresh(tab.source) };
          const hasResults = tab.result !== null;
          return <div className="document-panel" key={tab.id} hidden={activeId !== tab.id} role="tabpanel" id={`document-panel-${tab.id}`} aria-labelledby={`document-tab-${tab.id}`}>
            {tab.kind === "console" && <div className="console-workspace">
              <div className="console-toolbar"><div className="console-context"><label className="console-database-control console-schema-control"><span>{profile.namespace}</span><SelectField aria-label={`${profile.namespace} for ${tab.title}`} value={consoleSchema(tab.schema, tabCatalog)} disabled={busy || !tabCatalog?.schemas.length || !supportsConsoleSchema(tabSource?.dialect ?? "")} title={supportsConsoleSchema(tabSource?.dialect ?? "") ? `${profile.namespace} used for queries` : `${profile.namespace} is set by the connection`} onValueChange={schema => patchTab(tab.id, { schema, result: null, error: undefined })} placeholder={busy ? "Loading schemas…" : "No schemas available"} options={tabCatalog?.schemas.map(schema => ({ value: schema.name, label: schema.name })) ?? []} /></label><span className="dialect-label">{profile.language}</span></div><div className="console-actions tw:flex tw:items-center tw:gap-[16px]"><span className="inline-ai-hint tw:text-muted-foreground tw:text-label tw:whitespace-nowrap"><kbd>{IS_MAC ? "⌘ K" : "Ctrl K"}</kbd> to open inline AI</span><button className="btn primary run-button" disabled={busy} onClick={() => void runTab(tab.id)}>{runningTab === tab.id ? <span className="spinner" aria-hidden /> : <Icon name="play" size={13} />} Run <kbd>{RUN_SHORTCUT}</kbd></button></div></div>
              <QueryHelp dialect={tabSource?.dialect ?? ""} catalog={tabCatalog} schema={consoleSchema(tab.schema, tabCatalog)} onInsert={query => { const editor = editorHandle(tab.id).current; if (!editor) return; const text = editor.text(); const insert = `${text && !text.endsWith("\n") ? "\n" : ""}${query}\n`; if (editor.replaceRange) editor.replaceRange(text.length, text.length, insert); else editor.setText(text + insert); patchTab(tab.id, { sql: editor.text() }); }} />
              <div className="panes" ref={paneHandle(tab.id)}><div className="pane editor-pane" style={{ flex: hasResults ? `${tab.split} 1 0` : "1 1 0" }}><Editor preferences={{ ...editorPreferences, fontSize: DEFAULT_EDITOR_PREFERENCES.fontSize }} initial={tab.sql} onChange={() => persistSession.current()} handle={editorHandle(tab.id)} dialect={tabSource?.dialect} catalog={tabCatalog} schema={consoleSchema(tab.schema, tabCatalog)} />{inlineTarget?.tabId === tab.id && activeId === tab.id && <InlineSqlAssistant target={inlineTarget} dialect={tabSource?.dialect ?? ""} source={tab.source} schema={consoleSchema(tab.schema, tabCatalog)} catalog={tabCatalog} error={tab.error} onClose={() => { setInlineTarget(null); editorHandle(tab.id).current?.focus?.(); }} onSettings={() => setSettingsSection("ai")} onApply={(sql, action) => {
                  const editor = editorHandle(tab.id).current;
                  if (!editor || editor.text() !== inlineTarget.fullSql) return false;
                  const from = action === "fix" && !inlineTarget.selectedSql ? 0 : inlineTarget.from;
                  const to = action === "fix" && !inlineTarget.selectedSql ? inlineTarget.fullSql.length : inlineTarget.to;
                  if (editor.replaceRange) editor.replaceRange(from, to, sql);
                  else editor.setText(inlineTarget.fullSql.slice(0, from) + sql + inlineTarget.fullSql.slice(to));
                  patchTab(tab.id, { sql: editor.text(), error: undefined }); setInlineTarget(null); return true;
                }} />}</div>{hasResults && <><Splitter container={paneHandle(tab.id)} value={tab.split} onChange={(split) => patchTab(tab.id, { split })} /><div className="pane result-pane" style={{ flex: `${1 - tab.split} 1 0` }}><div className="result-toolbar"><div className="result-switch"><button className={tab.resultView === "table" ? "active" : ""} onClick={() => patchTab(tab.id, { resultView: "table" })}><Icon name="table" size={13} /> Results <span>{tab.result!.rows.length.toLocaleString()}</span></button>{profile.documentView && <button className={tab.resultView === "json" ? "active" : ""} onClick={() => patchTab(tab.id, { resultView: "json" })}>JSON</button>}<button className={tab.resultView === "chart" ? "active" : ""} onClick={() => patchTab(tab.id, { resultView: "chart" })}><Icon name="chart" size={13} /> Chart</button></div><div className="result-actions tw:flex tw:items-center tw:gap-[10px] tw:min-w-0"><span className="result-summary" title={tab.result!.summary}>{tab.result!.summary}</span>{tab.result!.row_limit && <button type="button" className="btn ghost" disabled={busy} onClick={() => void showAllRows(tab.id)}>Run full query</button>}<button className="icon-button" aria-label="Export displayed results as CSV" onClick={() => void exportTab(tab)}><Icon name="download" size={14} /></button></div></div><div className="result-body tw:[flex:1] tw:min-h-0 tw:overflow-hidden">{tab.resultView === "json" && tab.result ? <ResultDocuments result={tab.result} /> : tab.resultView === "table" ? <ResultGrid result={tab.result} {...resultEditing(tab)} /> : <ResultChart result={tab.result} />}</div></div></>}</div>
              {tab.error && <ErrorNotice error={tab.error} title="Query failed" context={tab.source} onRetry={() => void runTab(tab.id)} busy={busy} />}{!hasResults && <div className="results-idle"><Icon name="table" size={13} /><span>Results appear here after you run a query.</span><kbd>{RUN_SHORTCUT}</kbd></div>}
            </div>}
            {tab.kind === "table" && <TableWorkspace dialect={tabSource?.dialect} tab={tab} metadata={tabCatalog?.schemas.find((item) => item.name === tab.schema)?.tables.find((item) => item.name === tab.table) ?? null} connected={liveConnections.includes(tab.source)} busy={busy} onRefresh={() => void runTab(tab.id)} onShowAll={() => void showAllRows(tab.id)} onOpenConsole={() => void openView(tab.source, "console", { sql: tab.sql, schema: tab.schema }, true)} onExport={() => void exportTab(tab)} onResultView={(resultView) => patchTab(tab.id, { resultView })} {...resultEditing(tab)} />}
            {tab.kind === "dashboard" && <SourceDashboard {...props} history={history.filter((entry) => entry.source === tab.source)} onNavigate={(kind) => void openView(tab.source, kind)} />}
            {tab.kind === "schema-map" && <SchemaMap {...props} />}
            {(tab.kind === "databases" || tab.kind === "schema") && <DatabaseBrowser {...props} initialSchema={tab.kind === "schema" ? tab.schema : undefined} />}
            {tab.kind === "history" && <QueryHistory history={history} source={tab.source} onUse={(sql) => void openView(tab.source, "console", { sql }, true)} onClear={() => setHistory((before) => before.filter((entry) => entry.source !== tab.source))} />}
          </div>;
        })}
      </main>
      {assistantOpen ? <SqlAssistant settingsVersion={aiSettingsVersion} key={active?.id ?? "empty"} sql={active?.sql ?? ""} getSql={() => active ? editorHandle(active.id).current?.text() ?? active.sql : ""} enabled={active?.kind === "console"} source={active?.source ?? selected ?? ""} onOpenConsole={() => { const name = active?.source ?? selected; if (name) void openView(name, "console"); else setAdding("source"); }} title={active?.title ?? "SQL console"} dialect={sources.find(item => item.name === active?.source)?.dialect ?? ""} schema={active ? consoleSchema(active.schema, catalogs[active.source] ?? null) : ""} catalog={active ? catalogs[active.source] ?? null : null} error={active?.error} onClose={() => setAssistantOpen(false)} onSettings={() => setSettingsSection("ai")} onUse={sql => { if (!active || active.kind !== "console") return; editorHandle(active.id).current?.setText(sql); patchTab(active.id, { sql, error: undefined }); }} /> : <div className="ai-assistant-rail"><button type="button" className="icon-button" aria-label="Open AI assistant sidebar" title="AI assistant" onClick={() => setAssistantOpen(true)}><Icon name="ai" size={17} /></button></div>}
    </div>
    {message.kind === "error" && message.text !== active?.error && <ErrorNotice error={message.text} />}
    {exportedPath && <ExportToast path={exportedPath} onClose={closeExportToast} onReveal={() => void api.revealFile(exportedPath).catch((e: unknown) => setMessage({ kind: "error", text: errorText(e) }))} />}
    <footer className="statusbar"><span className="connection"><span className={`connection-dot${connected && sources.some((item) => item.name === connected) ? " online" : ""}`} />{connected && sources.some((item) => item.name === connected) ? connected : "No active connection"}</span><span className={`message ${message.kind}`} role="status" title={message.text || "Ready"}>{message.text || "Ready"}</span><AppUpdate /><span className="status-end">{tabs.length} tabs{catalog ? ` · ${catalog.schemas.reduce((n, item) => n + item.tables.length, 0)} objects` : ""}</span></footer>
    {settingsSection && <SettingsDialog initialSection={settingsSection} theme={theme} editorPreferences={editorPreferences} onSaveGeneral={(nextTheme, preferences) => {
      localStorage.setItem(EDITOR_PREFERENCES_KEY, JSON.stringify(preferences));
      localStorage.setItem("rowcraft.theme", nextTheme);
      setTheme(nextTheme); setEditorPreferences(preferences);
    }} onClose={() => { setSettingsSection(null); setAiSettingsVersion(value => value + 1); }} />}
    {(adding || editingSource) && <AddSourceDialog sourceName={editingSource ?? undefined} key={editingSource ?? "new"} initialDriver={adding === "sqlite" ? "sqlite" : undefined} initialMode={adding === "driver" ? "drivers" : "sources"} onClose={() => { setAdding(null); setEditingSource(null); }} onAdded={editingSource ? updatedSource : added} />}
  </div>;
}
