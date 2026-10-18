import { useRef, useState } from "react";
import type { Workspace } from "../workspaces";
import { Icon } from "./Icon";

interface Props {
  workspaces: Workspace[];
  activeId: string;
  busy: boolean;
  onSwitch(id: string): void;
  onCreate(name: string): void;
  onRename(name: string): void;
}

export function WorkspaceSwitcher({ workspaces, activeId, busy, onSwitch, onCreate, onRename }: Props) {
  const menu = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const [editing, setEditing] = useState<"create" | "rename" | null>(null);
  const [name, setName] = useState("");
  const [position, setPosition] = useState({ top: 0, left: 0 });
  const active = workspaces.find((workspace) => workspace.id === activeId)!;
  const duplicate = workspaces.some((workspace) => (editing !== "rename" || workspace.id !== activeId) && workspace.name.toLowerCase() === name.trim().toLowerCase());
  const beginEditing = (mode: "create" | "rename") => {
    setEditing(mode); setName(mode === "rename" ? active.name : "");
    requestAnimationFrame(() => { input.current?.focus(); input.current?.select(); });
  };
  const close = () => { menu.current?.hidePopover(); trigger.current?.focus(); };
  return <>
    <button ref={trigger} className="workspace-switcher" aria-label={`Switch workspace, current: ${active.name}`} title="Switch or create workspace" disabled={busy} popoverTarget="workspace-switcher-menu" onClick={(event) => {
      const box = event.currentTarget.getBoundingClientRect();
      setPosition({ top: box.bottom + 6, left: box.left });
    }}><img src="/brand/svg/mark-dark.svg" width={24} height={18} alt="" className="tw:block tw:dark:hidden tw:shrink-0" /><img src="/brand/svg/mark-white.svg" width={24} height={18} alt="" className="tw:hidden tw:dark:block tw:shrink-0" /><span>{active.name}</span><Icon name="chevronDown" size={12} /></button>
    <div ref={menu} id="workspace-switcher-menu" popover="auto" className="dropdown-menu workspace-switcher-menu" style={position} onToggle={(event) => {
      if (event.newState === "closed") { setEditing(null); setName(""); }
    }}>
      <div className="menu-label tw:[padding:8px_9px] tw:text-caption tw:text-muted-foreground">Workspaces</div>
      <div className="workspace-options">{workspaces.map((workspace) => <button key={workspace.id} disabled={busy} aria-current={workspace.id === activeId ? "true" : undefined} onClick={() => { close(); onSwitch(workspace.id); }}>
        <Icon name={workspace.id === activeId ? "check" : "database"} size={14} /><span>{workspace.name}<small>{workspace.sources.length} data {workspace.sources.length === 1 ? "source" : "sources"}</small></span>
      </button>)}</div>
      {editing ? <form className="workspace-create-form" onSubmit={(event) => { event.preventDefault(); if (!name.trim() || duplicate || busy) return; if (editing === "rename") onRename(name.trim()); else onCreate(name.trim()); setEditing(null); setName(""); close(); }}>
        <label htmlFor="workspace-name">Workspace name</label>
        <input ref={input} id="workspace-name" className="input" maxLength={64} value={name} onChange={(event) => setName(event.target.value)} aria-describedby={duplicate ? "workspace-name-error" : undefined} aria-invalid={duplicate || undefined} />
        {duplicate && <small id="workspace-name-error" role="alert">Choose a different workspace name.</small>}
        <div className="row"><button type="button" className="btn" onClick={() => { setEditing(null); setName(""); }}>Cancel</button><button className="btn primary" type="submit" disabled={!name.trim() || duplicate || busy}>{editing === "rename" ? "Save name" : "Create workspace"}</button></div>
      </form> : <><button className="workspace-create-action" disabled={busy} onClick={() => beginEditing("rename")}><Icon name="settings" size={14} /><span>Rename workspace</span></button><button disabled={busy} onClick={() => beginEditing("create")}><Icon name="plus" size={14} /><span>Create workspace</span></button></>}
    </div>
  </>;
}
