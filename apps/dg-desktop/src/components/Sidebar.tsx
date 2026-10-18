import { getAppScale } from "../preferences";
import { databaseProfile } from "../database-profile";
import { useEffect, useId, useRef, useState } from "react";
import { ContextMenu } from "radix-ui";
import { api, errorText } from "../api";
import { ConnectionMenu, ConnectionMenuItems, type ConnectionAction } from "./ConnectionMenu";
import type { Catalog, SourceSummary } from "../api";
import type { WorkspaceView } from "../workbench-model";
import { WorkspaceSwitcher } from "./WorkspaceSwitcher";
import type { Workspace } from "../workspaces";
import { Explorer } from "./Explorer";
import { Icon, type IconName } from "./Icon";
import { OpenEditors } from "./OpenEditors";
import type { WorkspaceTab } from "../workspace-tabs";

export const WORKSPACE_VIEWS: { id: WorkspaceView; label: string; icon: IconName }[] = [
  { id: "dashboard", label: "Overview", icon: "dashboard" },
  { id: "console", label: "SQL console", icon: "console" },
  { id: "schema-map", label: "Schema map", icon: "map" },
  { id: "databases", label: "Databases", icon: "database" },
  { id: "history", label: "Query history", icon: "history" },
];
const SIDEBAR_WIDTH_KEY = "dglite.sidebar-width";
const MIN_WIDTH = 160;
const MAX_WIDTH = 600;
function savedWidth(): number | null {
  try {
    const value = Number(localStorage.getItem(SIDEBAR_WIDTH_KEY));
    return Number.isFinite(value) && value >= MIN_WIDTH && value <= MAX_WIDTH ? value : null;
  } catch { return null; }
}
function viewsFor(dialect: string) {
  const profile = databaseProfile(dialect);
  return WORKSPACE_VIEWS.map(view => ({...view, label: view.id === "console" ? profile.console : view.id === "schema-map" && !profile.relationships ? "Object map" : view.label}));
}
interface Props {
  tabs: WorkspaceTab[];
  activeTabId: string | null;
  onSelectTab(id: string): void;
  onCloseTab(id: string): void;
  onKeepTab?(id: string): void;
  onSaveAllEditors?(): void;
  onCloseAllEditors?(): void;
  onMoveSource?(name: string, workspaceId: string): void;
  workspaces: Workspace[];
  workspaceId: string;
  onSwitchWorkspace(id: string): void;
  onCreateWorkspace(name: string): void;
  onRenameWorkspace(name: string): void;
  sources: SourceSummary[];
  selected: string | null;
  connected: string | null;
  connectedSources?: readonly string[];
  view: WorkspaceView;
  busy: boolean;
  onNavigate(name: string, view: WorkspaceView): void;
  onAdd(kind?: "source" | "sqlite" | "driver"): void;
  onRemove(name: string): void;
  onEdit(name: string): void;
  catalog: Catalog | null;
  onOpenTable(schema: string, table: string): void;
  onOpenSchema?(schema: string): void;
  onRefresh(): void;
  onRefreshSource(name: string): void;
  onReconnect(name: string): void;
  onNewConsole(name: string): void;
  hidden: boolean;
}
export function Sidebar({ tabs, activeTabId, onSelectTab, onCloseTab, onKeepTab, onSaveAllEditors, onCloseAllEditors, onMoveSource, workspaces, workspaceId, onSwitchWorkspace, onCreateWorkspace, onRenameWorkspace, sources, selected, connected, connectedSources, view, busy, onNavigate, onAdd, onRemove, onEdit, catalog, onOpenTable, onOpenSchema, onRefresh, onRefreshSource, onReconnect, onNewConsole, hidden }: Props) {
  const sidebar = useRef<HTMLElement>(null);
  const [connectionsOpen, setConnectionsOpen] = useState(true);
  const connectionsId = useId();
  const [width, setWidth] = useState(savedWidth);
  const [resizing, setResizing] = useState(false);
  const drag = useRef<{ pointerId: number; x: number; width: number; latest: number } | null>(null);
  const maxWidth = () => Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, window.innerWidth / getAppScale() - 320));
  const resize = (value: number, persist = false) => {
    const next = Math.max(MIN_WIDTH, Math.min(maxWidth(), value));
    setWidth(next);
    if (persist) { try { localStorage.setItem(SIDEBAR_WIDTH_KEY, String(next)); } catch { /* Resizing still works without storage. */ } }
    return next;
  };
  const finishResize = () => {
    if (drag.current) resize(drag.current.latest, true);
    drag.current = null; setResizing(false);
  };
  const resetWidth = () => {
    setWidth(null);
    try { localStorage.removeItem(SIDEBAR_WIDTH_KEY); } catch { /* Keep the responsive default for this session. */ }
  };
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const addMenu = useRef<HTMLDivElement>(null);
  const [notice, setNotice] = useState<{ error: boolean; text: string } | null>(null);
  async function copy(name: string, connectionString = false) {
    try {
      const text = connectionString ? (await api.sourceProperties(name)).connection_string : name;
      await navigator.clipboard.writeText(text);
      setNotice({ error: false, text: connectionString ? "Connection string copied." : "Connection name copied." });
    } catch (error) { setNotice({ error: true, text: errorText(error) }); }
  }
  const connectionActions = (source: SourceSummary): ConnectionAction[] => [
    { label: "Properties…", icon: "settings", run: () => onEdit(source.name), disabled: busy },
    { label: "Copy connection string", icon: "link", run: () => void copy(source.name, true) },
    { label: "Move to workspace", icon: "arrow", run: () => {}, disabled: busy || !onMoveSource || workspaces.length < 2,
      children: workspaces.filter(item => item.id !== workspaceId).map(item => ({ label: item.name, icon: "database", run: () => onMoveSource?.(source.name, item.id) })) },
    { label: "Copy connection name", icon: "copy", run: () => void copy(source.name) },
    ...viewsFor(source.dialect).map((item, index) => ({ label: item.label, icon: item.icon, run: () => onNavigate(source.name, item.id), disabled: busy, separator: index === 0 })),
    { label: `New ${databaseProfile(source.dialect).console}`, icon: "plus", run: () => onNewConsole(source.name), disabled: busy, separator: true },
    { label: (connectedSources ? connectedSources.includes(source.name) : connected === source.name) ? "Reconnect" : "Connect", icon: "play", run: () => onReconnect(source.name), disabled: busy },
    { label: "Refresh database objects", icon: "refresh", run: () => onRefreshSource(source.name), disabled: busy },
    { label: expanded.has(source.name) ? "Collapse connection" : "Expand connection", icon: expanded.has(source.name) ? "chevronDown" : "chevronRight", run: () => toggle(source.name) },
    { label: "Open a data source…", icon: "database", run: () => onAdd(), disabled: busy, separator: true },
    { label: "Remove data source", icon: "minus", run: () => onRemove(source.name), disabled: busy, destructive: true, separator: true },
  ];
  const toggle = (name: string) => setExpanded((before) => {
    const next = new Set(before);
    if (!next.delete(name)) next.add(name);
    return next;
  });
  useEffect(() => {
    if (selected) setExpanded((before) => new Set(before).add(selected));
  }, [selected]);
  const filtered = sources.filter((s) => s.name.toLowerCase().includes(search.trim().toLowerCase()));
  return <><ContextMenu.Root><ContextMenu.Trigger asChild><aside id="workspace-sidebar" ref={sidebar} className={`sidebar${hidden ? " sidebar-hidden" : ""}`} aria-label="Data sources" style={width === null ? undefined : { width, minWidth: MIN_WIDTH, maxWidth: "max(160px, calc(100vw - 320px))" }}>
    <div className="sidebar-head"><WorkspaceSwitcher workspaces={workspaces} activeId={workspaceId} busy={busy} onSwitch={onSwitchWorkspace} onCreate={onCreateWorkspace} onRename={onRenameWorkspace} /><span className="source-count tw:text-muted-foreground tw:text-meta">{sources.length}</span>
      <button className="icon-button" title="Add a data source" aria-label="Add a data source" popoverTarget="add-source-menu"><Icon name="plus" /></button>
      <div popover="auto" id="add-source-menu" ref={addMenu} className="dropdown-menu">
        <div className="menu-label tw:[padding:8px_9px] tw:text-caption tw:text-muted-foreground">Add to workspace</div>
        <button onClick={() => { addMenu.current?.hidePopover(); onAdd(); }}><Icon name="database" /><span>Data source<small>Connect a database server</small></span></button>
        <button onClick={() => { addMenu.current?.hidePopover(); onAdd("sqlite"); }}><Icon name="file" /><span>SQLite file<small>Import an existing database</small></span></button>
        <button onClick={() => { addMenu.current?.hidePopover(); onAdd("driver"); }}><Icon name="settings" /><span>Custom driver<small>JDBC or a native protocol</small></span></button>
      </div>
    </div>
    <OpenEditors onKeep={onKeepTab} tabs={tabs} activeId={activeTabId} busy={busy} onSelect={onSelectTab} onClose={onCloseTab} onNew={selected ? () => onNewConsole(selected) : undefined} onSaveAll={onSaveAllEditors} onCloseAll={onCloseAllEditors} />
    <button type="button" id="connections-toggle" className="open-editors-heading connections-heading" aria-expanded={connectionsOpen} aria-controls={connectionsId} onClick={() => setConnectionsOpen(value => !value)}>
      <Icon name={connectionsOpen ? "chevronDown" : "chevronRight"} size={13} /><span>Connections</span>
    </button>
    <div id={connectionsId} className="connections-panel" hidden={!connectionsOpen}>
    <div className="source-search"><Icon name="search" /><input id="source-search" aria-label="Find a data source" placeholder="Find a data source…" value={search} onChange={(e) => setSearch(e.target.value)} /><kbd>⌘ K</kbd></div>
    <div className="sidebar-scroll tw:[flex:1] tw:overflow-auto tw:min-h-0">
      {!sources.length && <div className="sidebar-empty tw:[padding:0_16px] tw:text-muted-foreground tw:leading-[1.6]"><p>Your workspace is empty.</p><button className="text-button" onClick={() => onAdd()}>Add your first data source</button></div>}
      {!!sources.length && !filtered.length && <p className="sidebar-empty tw:[padding:0_16px] tw:text-muted-foreground tw:leading-[1.6]">No matching data sources.</p>}
      <ul className="source-list">{filtered.map((source) => {
        const active = selected === source.name;
        const open = expanded.has(source.name);
        return <li key={source.name} className={`source-group${active ? " current" : ""}`}>
          <ConnectionMenu name={source.name} actions={connectionActions(source)}>
            <button className="source-expand icon-button" aria-label={`${open ? "Collapse" : "Expand"} ${source.name}`} aria-expanded={open} onClick={() => toggle(source.name)}><Icon name={open ? "chevronDown" : "chevronRight"} size={13} /></button>
            <button className={`source${active ? " active" : ""}`} disabled={busy} onClick={() => onNavigate(source.name, "dashboard")} aria-current={active} title={source.name}>
              <span className={`source-emblem ${source.dialect === "sqlite" ? "sqlite" : ""}`}><Icon name="database" size={17} /></span>
              <span className="source-text"><span className="source-name">{source.name}</span><span className="source-dialect" title={source.host ?? "Local file"}>{source.dialect}</span></span>
              <span className={`connection-dot${(connectedSources ? connectedSources.includes(source.name) : connected === source.name) ? " online" : ""}`} style={source.color ? { background: source.color } : undefined} title={(connectedSources ? connectedSources.includes(source.name) : connected === source.name) ? "Connected" : "Not connected"} />
            </button>
          </ConnectionMenu>
          {open && <div className="source-children"><nav aria-label={`${source.name} views`}>{viewsFor(source.dialect).map((item) => <button key={item.id} className={`source-nav-item${active && view === item.id ? " selected" : ""}`} disabled={busy && !active} aria-current={active && view === item.id ? "page" : undefined} onClick={() => onNavigate(source.name, item.id)}><Icon name={item.icon} /><span>{item.label}</span>{item.id === "databases" && active && catalog && <span className="nav-count tw:ml-auto tw:text-caption tw:text-muted-foreground">{catalog.schemas.length}</span>}</button>)}</nav>
          {active && (connectedSources ? connectedSources.includes(source.name) : connected === source.name) && catalog && <Explorer key={source.name} catalog={catalog} dialect={source.dialect} sourceName={source.name} busy={busy} onRefresh={onRefresh} onOpenTable={onOpenTable} onOpenSchema={onOpenSchema} />}
          {active && busy && !catalog && <div className="tree-loading" aria-label="Reading database objects"><span /><span /><span /></div>}
          </div>}
        </li>;
      })}</ul>
    </div>
    </div>
    {notice && <div className="sidebar-action-notice" role={notice.error ? "alert" : "status"}>{notice.text}<button type="button" className="icon-button" aria-label="Dismiss connection message" onClick={() => setNotice(null)}><Icon name="close" size={12} /></button></div>}
    <div className="sidebar-resizer" role="separator" tabIndex={0} aria-label="Resize data sources sidebar" aria-orientation="vertical" aria-valuemin={MIN_WIDTH} aria-valuemax={maxWidth()} aria-valuenow={Math.round(Math.min(width ?? (sidebar.current ? sidebar.current.getBoundingClientRect().width / getAppScale() || 244 : 244), maxWidth()))} aria-valuetext={`${Math.round(Math.min(width ?? (sidebar.current ? sidebar.current.getBoundingClientRect().width / getAppScale() || 244 : 244), maxWidth()))} pixels`} title="Drag to resize · Double-click to reset" data-resizing={resizing || undefined}
      onDoubleClick={resetWidth}
      onPointerDown={event => {
        if (event.button !== 0 || !sidebar.current) return;
        event.preventDefault(); event.stopPropagation();
        const initial = sidebar.current.getBoundingClientRect().width / getAppScale();
        drag.current = { pointerId: event.pointerId, x: event.clientX, width: initial, latest: initial };
        event.currentTarget.setPointerCapture(event.pointerId); setResizing(true);
      }}
      onPointerMove={event => {
        if (!drag.current || drag.current.pointerId !== event.pointerId) return;
        drag.current.latest = resize(drag.current.width + (event.clientX - drag.current.x) / getAppScale());
      }}
      onPointerUp={finishResize} onPointerCancel={finishResize} onLostPointerCapture={finishResize}
      onKeyDown={event => {
        if (!["ArrowLeft", "ArrowRight", "Home", "End", "Enter"].includes(event.key)) return;
        event.preventDefault(); event.stopPropagation();
        if (event.key === "Enter") { resetWidth(); return; }
        const current = (sidebar.current ? sidebar.current.getBoundingClientRect().width / getAppScale() : 0) || width || 244;
        resize(event.key === "Home" ? MIN_WIDTH : event.key === "End" ? maxWidth() : current + (event.key === "ArrowRight" ? 1 : -1) * (event.shiftKey ? 40 : 10), true);
      }} />
  </aside></ContextMenu.Trigger>
    <ContextMenu.Portal><ContextMenu.Content className="connection-menu" collisionPadding={8} aria-label="Workspace actions">
      <ContextMenu.Label className="connection-menu-label">{workspaces.find(workspace => workspace.id === workspaceId)?.name}</ContextMenu.Label>
      <ConnectionMenuItems context actions={[
        { label: "Open a data source…", icon: "database", run: () => onAdd(), disabled: busy },
        { label: "Workspaces…", icon: "dashboard", run: () => requestAnimationFrame(() => document.querySelector<HTMLButtonElement>(".workspace-switcher")?.click()), disabled: busy },
        ...(sources.find(source => source.name === selected) ? connectionActions(sources.find(source => source.name === selected)!).filter(action => action.label !== "Open a data source…") : []),
        ...workspaces.filter(workspace => workspace.id !== workspaceId).map((workspace, index) => ({ label: `Switch to ${workspace.name}`, icon: "database" as const, run: () => onSwitchWorkspace(workspace.id), disabled: busy, separator: index === 0 })),
      ]} />
    </ContextMenu.Content></ContextMenu.Portal>
  </ContextMenu.Root></>;
}
