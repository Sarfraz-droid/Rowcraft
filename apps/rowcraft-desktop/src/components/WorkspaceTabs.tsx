import { databaseProfile } from "../database-profile";
import { useEffect, useId, useRef } from 'react';
import type { KeyboardEvent } from 'react';
import type { WorkspaceTab } from '../workspace-tabs';
import type { WorkspaceView } from '../workbench-model';
import { Icon, type IconName } from './Icon';

interface Props {
  dialect?: string; tabs: WorkspaceTab[]; activeId: string | null; busy: boolean;
  onSelect(id: string): void; onClose(id: string): void; onKeep?(id: string): void;
  onAdd(kind: WorkspaceView | 'schema'): void;
}
const options: { kind: WorkspaceView | 'schema'; name: string; detail: string }[] = [
  { kind: 'console', name: 'SQL console', detail: 'Independent query editor' },
  { kind: 'dashboard', name: 'Overview', detail: 'Source inventory and activity' },
  { kind: 'databases', name: 'Databases', detail: 'Tables, columns, and indexes' },
  { kind: 'schema-map', name: 'Schema map', detail: 'Table relationships' },
  { kind: 'schema', name: 'Schema browser', detail: 'Browse database structure' },
  { kind: 'history', name: 'Query history', detail: 'Previously executed SQL' },
];
export function WorkspaceTabs({ dialect, tabs, activeId, busy, onSelect, onClose, onAdd, onKeep }: Props) {
  const profile = databaseProfile(dialect);
  const list = useRef<HTMLDivElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const addButton = useRef<HTMLButtonElement>(null);
  const menuId = useId();
  useEffect(() => {
    list.current?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [activeId]);
  const select = (index: number) => {
    const tab = tabs[index]; if (!tab) return;
    onSelect(tab.id);
    list.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[index]?.focus();
  };
  const handleKeys = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
      event.preventDefault(); select((index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length);
    } else if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault(); select(event.key === 'Home' ? 0 : tabs.length - 1);
    } else if (event.key === 'Delete' && !busy) {
      event.preventDefault(); onClose(tabs[index]!.id);
      const next = index === tabs.length - 1 ? index - 1 : index + 1;
      list.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus();
    }
  };
  return <div className="workspace-tabbar"><div ref={list} className="workspace-tabs tw:flex tw:overflow-x-auto tw:overflow-y-hidden tw:min-w-0 tw:items-stretch tw:[scrollbar-width:thin]" role="tablist" aria-label="Open workspace documents">
    {tabs.map((tab, index) => <div className={`workspace-tab ${tab.id === activeId ? 'is-active' : ''}${tab.preview ? ' is-preview' : ''}`} key={tab.id}>
      <button type="button" role="tab" id={`document-tab-${tab.id}`} aria-controls={`document-panel-${tab.id}`} aria-selected={tab.id === activeId} tabIndex={tab.id === activeId ? 0 : -1} className="workspace-tab-select" disabled={busy} onClick={() => onSelect(tab.id)} onDoubleClick={() => onKeep?.(tab.id)} onKeyDown={event => handleKeys(event, index)} title={`${tab.source} / ${tab.kind === 'table' && tab.schema ? `${tab.schema}.` : ''}${tab.title}`}>
        <Icon className="workspace-tab-symbol" name={({console:'console',table:'table',databases:'database','schema-map':'map',schema:'map',history:'history',dashboard:'dashboard'} as Record<string, IconName>)[tab.kind]!} size={14} />
        <span className="workspace-tab-title">{tab.title}</span><small>{tab.source}</small>
      </button>
      {tab.preview && <button type="button" className="workspace-tab-close" aria-label={`Keep ${tab.title} for ${tab.source} open`} onClick={() => onKeep?.(tab.id)} disabled={busy} title="Keep tab open (double-click tab)"><Icon name="check" size={12} /></button>}
      <button type="button" className="workspace-tab-close" aria-label={`Close ${tab.title} for ${tab.source}`} onClick={() => onClose(tab.id)} disabled={busy} title="Close tab"><Icon name="close" size={11} /></button>
    </div>)}
  </div><button type="button" ref={addButton} className="workspace-add-tab" disabled={busy} aria-label="New workspace tab" title="New tab" popoverTarget={menuId}><Icon name="plus" size={15} /></button>
  <div id={menuId} ref={menu} className="workspace-tab-menu" popover="auto" onToggle={event => {
    if (event.newState !== 'open' || !addButton.current || !menu.current) return;
    const anchor = addButton.current.getBoundingClientRect();
    const bounds = menu.current.getBoundingClientRect();
    menu.current.style.left = `${Math.max(8, Math.min(anchor.left, window.innerWidth - bounds.width - 8))}px`;
    menu.current.style.top = `${Math.min(anchor.bottom + 5, Math.max(8, window.innerHeight - bounds.height - 8))}px`;
    menu.current.querySelector<HTMLButtonElement>('button')?.focus();
  }} aria-label="Open a workspace view"><div className="workspace-tab-menu-heading tw:text-muted-foreground tw:text-meta tw:[padding:8px_10px_9px] tw:[border-bottom:1px_solid_var(--border)] tw:mb-[4px]">Open view</div>{options.map(option => <button type="button" key={option.kind} onClick={() => { menu.current?.hidePopover(); onAdd(option.kind); }}><span>{option.kind === "console" ? profile.console : option.kind === "schema-map" && !profile.relationships ? "Object map" : option.name}</span><small>{option.detail}</small></button>)}</div>
  </div>;
}
