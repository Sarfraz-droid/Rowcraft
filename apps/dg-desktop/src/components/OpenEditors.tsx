import { useId, useState } from "react";
import type { WorkspaceTab } from "../workspace-tabs";
import { Icon, type IconName } from "./Icon";

interface Props {
  tabs: WorkspaceTab[];
  activeId: string | null;
  busy: boolean;
  onSelect(id: string): void;
  onClose(id: string): void;
  onKeep?(id: string): void;
  onNew?(): void;
  onSaveAll?(): void;
  onCloseAll?(): void;
}
const icons: Record<WorkspaceTab["kind"], IconName> = {
  console: "console", table: "table", databases: "database", "schema-map": "map",
  schema: "map", history: "history", dashboard: "dashboard",
};

export function OpenEditors({ tabs, activeId, busy, onSelect, onClose, onKeep, onNew, onSaveAll, onCloseAll }: Props) {
  const [open, setOpen] = useState(true);
  const [search, setSearch] = useState("");
  const panelId = useId();
  const query = search.trim().toLowerCase();
  const filtered = tabs.filter(tab => `${tab.title} ${tab.source} ${tab.schema ?? ""}`.toLowerCase().includes(query));

  return <section className="open-editors" aria-label="Open editors">
    <div className="open-editors-header">
    <button type="button" className="open-editors-heading" aria-expanded={open} aria-controls={panelId} onClick={() => setOpen(value => !value)}>
      <Icon name={open ? "chevronDown" : "chevronRight"} size={13} />
      <span>Open editors</span>
    </button>
    <div className="open-editors-actions" role="toolbar" aria-label="Editor actions">
      <button type="button" className="icon-button" aria-label="New console" title="New console" disabled={busy || !onNew} onClick={onNew}><Icon name="newFile" size={15} /></button>
      <button type="button" className="icon-button" aria-label="Save all editors locally" title="Save all editors locally" disabled={busy || !tabs.length || !onSaveAll} onClick={onSaveAll}><Icon name="saveAll" size={15} /></button>
      <button type="button" className="icon-button" aria-label="Close all editors" title="Close all editors" disabled={busy || !tabs.length || !onCloseAll} onClick={onCloseAll}><Icon name="closeAll" size={15} /></button>
    </div>
    </div>
    <div id={panelId} hidden={!open}>
      {tabs.length > 0 && <div className="open-editors-search">
        <Icon name="search" size={13} />
        <input aria-label="Find an open editor" placeholder="Find a tab…" value={search} onChange={event => setSearch(event.target.value)} />
        {search && <button type="button" className="icon-button" aria-label="Clear editor search" onClick={() => setSearch("")}><Icon name="close" size={11} /></button>}
      </div>}
      <ul className="open-editors-list" aria-label="Open tabs">
        {filtered.map(tab => <li key={tab.id} className={`open-editor${tab.id === activeId ? " is-active" : ""}${tab.preview ? " is-preview" : ""}`}>
          <button type="button" className="icon-button open-editor-close" aria-label={`Close ${tab.title} for ${tab.source}`} title="Close tab" disabled={busy} onClick={() => onClose(tab.id)}><Icon name="close" size={12} /></button>
          <button type="button" className="open-editor-select" aria-current={tab.id === activeId ? "page" : undefined} disabled={busy} onClick={() => onSelect(tab.id)} onDoubleClick={() => onKeep?.(tab.id)} title={`${tab.preview ? "Preview · Double-click to keep open · " : ""}${tab.source} / ${tab.schema ? `${tab.schema}.` : ""}${tab.title}`}>
            <Icon name={icons[tab.kind]} size={14} />
            <span className="open-editor-label"><span>{tab.title}</span><small>{tab.source}{tab.schema ? ` / ${tab.schema}` : ""}</small></span>
          </button>
          {tab.preview && <button type="button" className="icon-button" aria-label={`Keep ${tab.title} for ${tab.source} open`} title="Keep tab open" disabled={busy} onClick={() => onKeep?.(tab.id)}><Icon name="check" size={12} /></button>}
        </li>)}
      </ul>
      {!filtered.length && <p className="open-editors-empty">{tabs.length ? "No matching tabs." : "No open editors."}</p>}
    </div>
  </section>;
}
