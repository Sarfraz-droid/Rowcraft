import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";

import type { Cell, QueryOutput, RowChanges } from "../api";
import { GUTTER_WIDTH, ROW_HEIGHT, cellLabel, columnWidth } from "../grid";
import { buildChanges, editText, noEdits, pendingCount, type EditSupport, type PendingEdits } from "../result-edit";
import { resultMarkdown } from "../result-tools";
import { Icon } from "./Icon";
import { SelectField } from "./SelectField";

const pageSizes = [25, 50, 100, 250].map(size => ({ value: String(size), label: String(size) }));
const NO_EDITS = noEdits();

export interface GridEditing extends EditSupport {
  /** Why nothing can be changed right now (busy, disconnected); shown on the disabled controls. */
  disabled?: string;
  /** Saves the changes; resolves to whether they were saved, once the result is reloaded. */
  onSave(changes: RowChanges): Promise<boolean>;
}

interface Props {
  result: QueryOutput | null;
  /** Lets rows be edited, added and deleted. Without it the grid is read-only. */
  editing?: GridEditing;
  /** Why the result is read-only (e.g. it joins tables); shown as a hint when `editing` is absent. */
  readOnly?: string;
}

const cloneEdits = (edits: PendingEdits): PendingEdits => ({
  updates: new Map([...edits.updates].map(([row, cells]) => [row, new Map(cells)])),
  deletes: new Set(edits.deletes),
  inserts: edits.inserts.map(cells => new Map(cells)),
});
const isBinary = (cell: Cell | undefined) => cell !== null && typeof cell === "object";
const plural = (count: number, word: string) => `${word}${count === 1 ? "" : "s"}`;

/** Only the rows in view are in the DOM, so a large result scrolls as smoothly as a small one. */
export function ResultGrid({ result, editing, readOnly }: Props) {
  const scroller = useRef<HTMLDivElement>(null);
  const rows = result?.rows ?? [];
  // Edits belong to one result: a rerun or reload starts clean.
  const [edits, setEdits] = useState<{ result: QueryOutput | null; value: PendingEdits }>({ result, value: NO_EDITS });
  const pending = edits.result === result ? edits.value : NO_EDITS;
  const changeCount = pendingCount(pending);
  const total = rows.length + pending.inserts.length;
  const [pageSize, setPageSize] = useState(25);
  const [pagination, setPagination] = useState({ result, page: 0 });
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  const page = pagination.result === result ? Math.min(pagination.page, pageCount - 1) : 0;
  const start = page * pageSize;
  const end = Math.min(start + pageSize, total);
  const goToPage = (next: number) => setPagination({ result, page: Math.max(0, Math.min(next, pageCount - 1)) });
  // Selection belongs to one result too.
  const [selection, setSelection] = useState<{ result: QueryOutput | null; rows: Set<number>; anchor: number | null }>({ result, rows: new Set(), anchor: null });
  const selected = selection.result === result ? selection.rows : new Set<number>();
  const anchor = selection.result === result ? selection.anchor : null;
  const [saving, setSaving] = useState(false);
  const [editor, setEditor] = useState<{ result: QueryOutput | null; row: number; col: number; text: string } | null>(null);
  const activeEditor = editor?.result === result ? editor : null;
  // The editor open right now, so a late blur from an input that already closed cannot commit into it.
  const editorRef = useRef(activeEditor);
  editorRef.current = activeEditor;
  const [menu, setMenu] = useState<{ x: number; y: number; row: number; col: number } | null>(null);
  const [copied, setCopied] = useState(false);
  /** Copies the selected rows (all rows when none are selected) as a Markdown table. */
  const copyMarkdown = () => {
    if (!result) return;
    void navigator.clipboard?.writeText(resultMarkdown(result, selected.size ? selected : undefined)).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };
  const select = (rows: Set<number>, nextAnchor: number | null) => setSelection({ result, rows, anchor: nextAnchor });
  const update = (change: (draft: PendingEdits) => void) => { const next = cloneEdits(pending); change(next); setEdits({ result, value: next }); };
  const clickRow = (index: number, event: MouseEvent) => {
    if (event.shiftKey && anchor !== null) {
      const next = event.metaKey || event.ctrlKey ? new Set(selected) : new Set<number>();
      for (let i = Math.min(anchor, index); i <= Math.max(anchor, index); i++) next.add(i);
      select(next, anchor);
    } else if (event.metaKey || event.ctrlKey) {
      const next = new Set(selected);
      if (!next.delete(index)) next.add(index);
      select(next, index);
    } else {
      select(selected.size === 1 && selected.has(index) ? new Set() : new Set([index]), index);
    }
  };
  const pageSelected = end > start && Array.from({ length: end - start }, (_, i) => start + i).every(i => selected.has(i));
  const togglePage = () => {
    const next = new Set(selected);
    for (let i = start; i < end; i++) { if (pageSelected) next.delete(i); else next.add(i); }
    select(next, null);
  };

  // Rows past the result are new rows, in the order they were added.
  const inserted = (row: number) => row >= rows.length;
  const cellAt = (row: number, col: number): { value: Cell; changed: boolean; placeholder: boolean } => {
    if (inserted(row)) {
      const cells = pending.inserts[row - rows.length];
      return cells?.has(col) ? { value: cells.get(col) ?? null, changed: true, placeholder: false } : { value: null, changed: false, placeholder: true };
    }
    const cells = pending.updates.get(row);
    return cells?.has(col) ? { value: cells.get(col) ?? null, changed: true, placeholder: false } : { value: rows[row]?.[col] ?? null, changed: false, placeholder: false };
  };
  const unavailable = !editing ? "This result is read-only." : saving ? "Saving changes…" : editing.disabled;
  const canEdit = (row: number, col: number) => !!editing && !unavailable && editing.editable[col]
    && (inserted(row) || (!editing.locked && !pending.deletes.has(row) && !isBinary(rows[row]?.[col])));
  const setCell = (row: number, col: number, value: Cell) => update(draft => {
    if (inserted(row)) { draft.inserts[row - rows.length]?.set(col, value); return; }
    const original = rows[row]?.[col] ?? null;
    const cells = draft.updates.get(row) ?? new Map<number, Cell>();
    const same = value === original || (value !== null && original !== null && !isBinary(original) && String(original) === value);
    if (same) cells.delete(col); else cells.set(col, value);
    if (cells.size) draft.updates.set(row, cells); else draft.updates.delete(row);
  });
  const startEdit = (row: number, col: number) => {
    if (!canEdit(row, col)) return;
    setMenu(null);
    setEditor({ result, row, col, text: editText(cellAt(row, col).value) });
  };
  const nextEditable = (row: number, col: number, step: 1 | -1) => {
    for (let c = col + step; c >= 0 && c < (result?.columns.length ?? 0); c += step) if (canEdit(row, c)) return c;
    return null;
  };
  /** Keeps the typed text, then moves to the neighbouring editable cell or closes the editor. */
  const commit = (at: { row: number; col: number }, move?: 1 | -1) => {
    const open = editorRef.current;
    if (!open || open.row !== at.row || open.col !== at.col) return;
    setCell(open.row, open.col, open.text);
    const next = move ? nextEditable(open.row, open.col, move) : null;
    editorRef.current = null;
    if (next === null) { setEditor(null); if (move) scroller.current?.focus(); }
    else setEditor({ result, row: open.row, col: next, text: editText(cellAt(open.row, next).value) });
  };
  const cancelEdit = () => { editorRef.current = null; setEditor(null); scroller.current?.focus(); };

  const selectedExisting = [...selected].filter(row => !inserted(row));
  const restoring = selectedExisting.length === selected.size && selected.size > 0 && selectedExisting.every(row => pending.deletes.has(row));
  const deleteBlocked = unavailable ?? (selectedExisting.length && editing?.locked ? editing.locked : undefined);
  /** Marks rows for deletion (or keeps them again); new rows are simply dropped. */
  const toggleDelete = () => {
    if (deleteBlocked) return;
    const indexes = [...selected];
    const dropped = new Set(indexes.filter(inserted).map(row => row - rows.length));
    update(draft => {
      for (const row of indexes.filter(row => !inserted(row))) { if (restoring) draft.deletes.delete(row); else draft.deletes.add(row); }
      draft.inserts = draft.inserts.filter((_, i) => !dropped.has(i));
    });
    select(new Set(), null);
  };
  const addRow = (values = new Map<number, Cell>()) => {
    if (!editing || unavailable) return;
    const row = total;
    update(draft => { draft.inserts.push(values); });
    setPagination({ result, page: Math.floor(row / pageSize) });
    select(new Set([row]), row);
    const first = editing.editable.findIndex(Boolean);
    if (first >= 0 && !values.size) setEditor({ result, row, col: first, text: "" });
  };
  /** A new row with this row's values, except its key, which the database or the user assigns. */
  const duplicateRow = (row: number) => {
    if (!editing) return;
    const values = new Map<number, Cell>();
    editing.editable.forEach((editable, col) => {
      const cell = cellAt(row, col);
      if (editable && !editing.key[col] && !cell.placeholder && !isBinary(cell.value)) values.set(col, cell.value);
    });
    addRow(values);
  };
  const revertRow = (row: number) => update(draft => { draft.updates.delete(row); draft.deletes.delete(row); });
  const discard = () => { editorRef.current = null; setEditor(null); setEdits({ result, value: noEdits() }); };
  const save = async () => {
    if (!editing || !result || !changeCount || unavailable) return;
    setSaving(true);
    try { if (await editing.onSave(buildChanges(result.columns, rows, pending))) { setEdits({ result, value: noEdits() }); select(new Set(), null); } }
    finally { setSaving(false); }
  };

  useEffect(() => {
    if (!menu) return;
    const close = (event: Event) => { if (!(event.target instanceof Element && event.target.closest(".grid-menu"))) setMenu(null); };
    document.addEventListener("mousedown", close);
    window.addEventListener("blur", close);
    return () => { document.removeEventListener("mousedown", close); window.removeEventListener("blur", close); };
  }, [menu]);
  useEffect(() => {
    if (scroller.current) scroller.current.scrollTop = 0;
  }, [result, page, pageSize]);
  const widths = useMemo(
    () => (result ? result.columns.map((name, i) => columnWidth(name, result.rows, i)) : []),
    [result],
  );
  const virtualizer = useVirtualizer({
    count: end - start,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
  });

  if (!result) {
    return <div className="grid-empty">Run a query to see its rows here.</div>;
  }
  if (result.columns.length === 0) {
    return <div className="grid-empty">The statement returned no rows.</div>;
  }

  const width = GUTTER_WIDTH + widths.reduce((a, b) => a + b, 0);
  const template = `${GUTTER_WIDTH}px ${widths.map((w) => `${w}px`).join(" ")}`;
  const deleteLabel = `${restoring ? "Restore" : "Delete"} ${plural(selected.size, "row")}`;
  const menuItem = (label: string, action: () => void, blocked?: string | false) => <button type="button" role="menuitem" disabled={!!blocked} title={blocked || undefined} onClick={() => { setMenu(null); action(); }}>{label}</button>;
  const menuCell = menu ? cellAt(menu.row, menu.col) : null;
  const menuBlocked = menu && !canEdit(menu.row, menu.col) ? unavailable ?? "This cell cannot be edited." : false;

  return (
    <div className="paginated-results">
    <div className="grid" ref={scroller} role="grid" tabIndex={0} aria-label="Query results" aria-multiselectable aria-rowcount={total + 1}
      onScroll={() => { if (menu) setMenu(null); }}
      onKeyDown={event => {
        const mod = event.metaKey || event.ctrlKey;
        if (event.key === "Escape" && (menu || selected.size)) { event.preventDefault(); if (menu) setMenu(null); else select(new Set(), null); }
        else if ((event.key === "Delete" || event.key === "Backspace") && selected.size && editing && !deleteBlocked) { event.preventDefault(); toggleDelete(); }
        else if (event.key === "a" && mod && total) { event.preventDefault(); select(new Set(Array.from({ length: total }, (_, i) => i)), null); }
        else if (event.key === "s" && mod && editing) { event.preventDefault(); void save(); }
        else if ((event.key === "Enter" || event.key === "F2") && selected.size === 1 && editing) {
          const [row = 0] = selected;
          const col = nextEditable(row, -1, 1);
          if (col !== null) { event.preventDefault(); startEdit(row, col); }
        }
      }}>
      <div className="grid-inner tw:min-w-full" style={{ width }}>
        <div className="grid-head" role="row" style={{ gridTemplateColumns: template, height: ROW_HEIGHT + 4 }}>
          <div className="grid-cell grid-gutter" role="columnheader"><input type="checkbox" className="grid-row-check" aria-label="Select all rows on this page" checked={pageSelected} onChange={togglePage} /></div>
          {result.columns.map((name, i) => (
            <div className={`grid-cell${editing && !editing.editable[i] ? " readonly" : ""}`} role="columnheader" key={i} title={editing && !editing.editable[i] ? `${name} — read-only: not a column of the edited table` : name}>
              {name}
            </div>
          ))}
        </div>
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const index = start + item.index;
            const isSelected = selected.has(index);
            const isNew = inserted(index);
            const isDeleted = pending.deletes.has(index);
            return (
            <div
              key={item.key}
              role="row"
              aria-rowindex={index + 2}
              aria-selected={isSelected}
              onClick={event => clickRow(index, event)}
              className={`grid-row${item.index % 2 ? " odd" : ""}${isSelected ? " selected" : ""}${isNew ? " inserted" : ""}${isDeleted ? " deleted" : ""}`}
              style={{
                gridTemplateColumns: template,
                height: item.size,
                transform: `translateY(${item.start}px)`,
              }}
            >
              <div className="grid-cell grid-gutter" role="rowheader" aria-label={String(index + 1)} title={isNew ? "New row, not saved yet" : isDeleted ? "Deleted when you save" : undefined}>
                <input type="checkbox" className="grid-row-check" tabIndex={-1} aria-label={`Select row ${index + 1}`} checked={isSelected} onClick={event => event.stopPropagation()} onChange={() => { const next = new Set(selected); if (!next.delete(index)) next.add(index); select(next, index); }} />
                <span>{isNew ? "+" : index + 1}</span>
              </div>
              {result.columns.map((name, c) => {
                if (activeEditor?.row === index && activeEditor.col === c) {
                  const at = { row: index, col: c };
                  return (
                    <div className="grid-cell editing" role="gridcell" key={c}>
                      <input
                        className="grid-cell-input"
                        aria-label={`Edit ${name}`}
                        autoFocus
                        value={activeEditor.text}
                        onFocus={event => event.currentTarget.select()}
                        onClick={event => event.stopPropagation()}
                        onDoubleClick={event => event.stopPropagation()}
                        onChange={event => {
                          const text = event.target.value;
                          if (editorRef.current) editorRef.current = { ...editorRef.current, text };
                          setEditor(open => open && { ...open, text });
                        }}
                        onBlur={() => commit(at)}
                        onKeyDown={event => {
                          // Keys typed here are text, not grid shortcuts (Backspace must not delete rows).
                          event.stopPropagation();
                          if (event.key === "Enter") { event.preventDefault(); commit(at); scroller.current?.focus(); }
                          else if (event.key === "Escape") { event.preventDefault(); cancelEdit(); }
                          else if (event.key === "Tab") { event.preventDefault(); commit(at, event.shiftKey ? -1 : 1); }
                        }}
                      />
                    </div>
                  );
                }
                const cell = cellAt(index, c);
                const label = cell.placeholder ? { text: "DEFAULT", dim: true, numeric: false } : cellLabel(cell.value);
                const cls = `grid-cell${label.dim ? " dim" : ""}${label.numeric ? " num" : ""}${cell.changed ? " edited" : ""}`;
                return (
                  <div className={cls} role="gridcell" key={c} title={label.text} aria-readonly={editing ? !canEdit(index, c) : undefined}
                    onDoubleClick={event => { event.stopPropagation(); startEdit(index, c); }}
                    onContextMenu={event => {
                      event.preventDefault();
                      if (!selected.has(index)) select(new Set([index]), index);
                      setMenu({ x: event.clientX, y: event.clientY, row: index, col: c });
                    }}>
                    {label.text}
                  </div>
                );
              })}
            </div>
            );
          })}
        </div>
      </div>
    </div>
    {menu && menuCell && <div className="grid-menu" role="menu" aria-label="Cell actions" style={{ left: menu.x, top: menu.y }} onKeyDown={event => { if (event.key === "Escape") { setMenu(null); scroller.current?.focus(); } }}>
      {editing && <>
        {menuItem("Edit value", () => startEdit(menu.row, menu.col), menuBlocked)}
        {menuItem("Set to NULL", () => setCell(menu.row, menu.col, null), menuBlocked)}
        {inserted(menu.row) && menuItem("Set to DEFAULT", () => update(draft => { draft.inserts[menu.row - rows.length]?.delete(menu.col); }), menuBlocked)}
        {!inserted(menu.row) && (pending.updates.has(menu.row) || pending.deletes.has(menu.row)) && menuItem("Revert row", () => revertRow(menu.row), unavailable)}
        <hr />
      </>}
      {menuItem("Copy value", () => void navigator.clipboard?.writeText(menuCell.value === null ? "NULL" : editText(menuCell.value)))}
      {menuItem(selected.size > 1 ? `Copy ${selected.size.toLocaleString()} rows as Markdown` : "Copy row as Markdown", copyMarkdown)}
      {editing && <>
        <hr />
        {menuItem("Add row", () => addRow(), unavailable)}
        {menuItem("Duplicate row", () => duplicateRow(menu.row), unavailable)}
        {menuItem(deleteLabel, toggleDelete, deleteBlocked)}
      </>}
    </div>}
    <nav className="result-pagination" aria-label="Result pagination">
      <div className="result-page-size"><span>Rows per page</span><SelectField aria-label="Rows per page" value={String(pageSize)} options={pageSizes} onValueChange={value => { setPageSize(Number(value)); setPagination({ result, page: 0 }); }} /></div>
      {editing && <button type="button" className="btn ghost" disabled={!!unavailable} title={unavailable ?? "Add a new row"} onClick={() => addRow()}><Icon name="plus" size={12} />Add row</button>}
      {rows.length > 0 && <button type="button" className="btn ghost" title={selected.size ? "Copy the selected rows as a Markdown table" : "Copy all rows as a Markdown table"} onClick={copyMarkdown}><Icon name="copy" size={12} />{copied ? "Copied" : selected.size ? "Copy selected as Markdown" : "Copy as Markdown"}</button>}
      {!editing && readOnly && <span className="result-readonly" title={readOnly}>Read-only</span>}
      {selected.size > 0 && <div className="result-selection" role="group" aria-label="Selected rows">
        <span className="tw:tabular-nums">{selected.size.toLocaleString()} selected</span>
        <button type="button" className="btn ghost" onClick={() => select(new Set(), null)}>Clear</button>
        {editing && <button type="button" className="btn ghost result-delete" disabled={!!deleteBlocked} title={deleteBlocked ?? (restoring ? "Keep the selected rows" : "Mark the selected rows for deletion")} onClick={toggleDelete}>
          <Icon name="minus" size={12} />{deleteLabel}
        </button>}
      </div>}
      {changeCount > 0 && <div className="result-changes" role="group" aria-label="Pending changes">
        <span className="tw:tabular-nums">{changeCount.toLocaleString()} pending {plural(changeCount, "change")}</span>
        <button type="button" className="btn ghost" disabled={saving} onClick={discard}>Discard</button>
        <button type="button" className="btn primary result-save" disabled={!!unavailable} title={unavailable ?? "Write the changes to the database (⌘S)"} onClick={() => void save()}>
          <Icon name="check" size={12} />{saving ? "Saving…" : "Save"}
        </button>
      </div>}
      <span className="result-row-range" role="status">{total ? `${(start + 1).toLocaleString()}–${end.toLocaleString()}` : "0"} of {total.toLocaleString()} rows</span>
      <div className="result-page-controls">
        <span>Page {page + 1} of {pageCount.toLocaleString()}</span>
        <button type="button" className="icon-button" aria-label="First page" title="First page" disabled={page === 0} onClick={() => goToPage(0)}><Icon name="chevronLeft" size={12} /><Icon name="chevronLeft" size={12} /></button>
        <button type="button" className="icon-button" aria-label="Previous page" title="Previous page" disabled={page === 0} onClick={() => goToPage(page - 1)}><Icon name="chevronLeft" size={14} /></button>
        <button type="button" className="icon-button" aria-label="Next page" title="Next page" disabled={page >= pageCount - 1} onClick={() => goToPage(page + 1)}><Icon name="chevronRight" size={14} /></button>
        <button type="button" className="icon-button" aria-label="Last page" title="Last page" disabled={page >= pageCount - 1} onClick={() => goToPage(pageCount - 1)}><Icon name="chevronRight" size={12} /><Icon name="chevronRight" size={12} /></button>
      </div>
    </nav>
    </div>
  );
}
