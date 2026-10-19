import { describe, expect, it } from 'vitest';
import { createWorkspaceTab, findExistingTab, closeWorkspaceTab, placeWorkspaceTab } from './workspace-tabs';
describe('workspace documents', () => {
  it('reuses one preview slot while preserving consoles and kept tabs', () => {
    const console = createWorkspaceTab('one', 'console'); console.sql = 'select keep_my_draft';
    const kept = createWorkspaceTab('one', 'dashboard');
    const preview = { ...createWorkspaceTab('one', 'table', {table:'people'}), preview: true };
    const next = createWorkspaceTab('two', 'history');
    const tabs = placeWorkspaceTab([console, preview, kept], next, true);
    expect(tabs).toEqual([console, { ...next, preview: true }, kept]);
    expect(tabs[0]?.sql).toBe('select keep_my_draft');
    expect(placeWorkspaceTab(tabs, createWorkspaceTab('one','console'), true)).toHaveLength(4);
    expect(placeWorkspaceTab(tabs, createWorkspaceTab('one','console'), true)[3]?.preview).toBe(false);
    expect(placeWorkspaceTab(tabs, createWorkspaceTab('one','databases'), false)).toHaveLength(4);
  });
  it('creates independent console documents without sharing results or SQL', () => {
    const a = createWorkspaceTab('production', 'console'); const b = createWorkspaceTab('production', 'console');
    a.sql = 'select 1'; a.result = { columns: ['n'], rows: [[1]], summary: '1 row' };
    expect(a.id).not.toBe(b.id); expect(b.sql).toBe(''); expect(b.result).toBeNull(); expect(findExistingTab([a], 'production', 'console')).toBeUndefined();
  });
  it('keeps objects with matching names in different sources or schemas separate', () => {
    const tabs = [createWorkspaceTab('one', 'table', {schema:'public',table:'users'}), createWorkspaceTab('two', 'table', {schema:'public',table:'users'}), createWorkspaceTab('one', 'table', {schema:'audit',table:'users'})];
    expect(findExistingTab(tabs,'two','table','public','users')).toBe(tabs[1]); expect(findExistingTab(tabs,'one','table','audit','users')).toBe(tabs[2]); expect(findExistingTab(tabs,'one','table','private','users')).toBeUndefined();
  });
  it('focuses the adjacent right document when the active document closes', () => {
    const tabs = [createWorkspaceTab('one','console'),createWorkspaceTab('one','dashboard'),createWorkspaceTab('one','history')];
    const closed = closeWorkspaceTab(tabs,tabs[1]!.id,tabs[1]!.id); expect(closed.activeId).toBe(tabs[2]!.id); expect(closed.tabs).toEqual([tabs[0],tabs[2]]); expect(tabs).toHaveLength(3);
  });
  it('selects the left document at the end, and clears selection for the last close', () => {
    const a=createWorkspaceTab('one','console'); const b=createWorkspaceTab('one','history'); expect(closeWorkspaceTab([a,b],b.id,b.id).activeId).toBe(a.id); expect(closeWorkspaceTab([a],a.id,a.id)).toEqual({tabs:[],activeId:null});
  });
  it('preserves active selection when a background document closes', () => {
    const a=createWorkspaceTab('one','console'); const b=createWorkspaceTab('one','history'); expect(closeWorkspaceTab([a,b],a.id,b.id).activeId).toBe(a.id); expect(closeWorkspaceTab([a,b],a.id,'missing').tabs).toEqual([a,b]);
  });
  it('distinguishes schema documents while retaining singleton source overviews', () => {
    const a=createWorkspaceTab('one','schema',{schema:'main'}); const b=createWorkspaceTab('one','dashboard'); expect(findExistingTab([a,b],'one','schema','other')).toBeUndefined(); expect(findExistingTab([a,b],'one','schema','main')).toBe(a); expect(findExistingTab([a,b],'one','dashboard')).toBe(b);
  });
});
