import { useRef, useState } from "react";
import type { Cell, QueryOutput } from "../api";
import { Icon } from "./Icon";

function structured(value: Cell) {
  if (typeof value === "string" && /^[\[{]/.test(value.trim())) {
    try { return JSON.parse(value) as unknown; } catch { /* Preserve BSON shell values and other text. */ }
  }
  return value;
}

function JsonValue({ value, label, path, depth = 0, comma = false }: {
  value: unknown; label?: string; path: string; depth?: number; comma?: boolean;
}) {
  const [open, setOpen] = useState(depth === 0);
  const compound = value !== null && typeof value === "object";
  const entries = compound ? Object.entries(value) : [];
  const array = Array.isArray(value);
  const prefix = label === undefined ? null : <span className="json-key">{JSON.stringify(label)}: </span>;
  const end = comma ? "," : "";
  if (!compound || !entries.length || depth >= 20) {
    const kind = value === null ? "null" : typeof value;
    return <div className="json-line">{prefix}<span className={`json-${kind}`}>{JSON.stringify(value)}</span>{end}</div>;
  }
  return <div className="json-branch">
    {depth === 0 ? <div className="json-line">{array ? "[" : "{"}</div> : <button type="button" className="json-disclosure" aria-label={`${open ? "Collapse" : "Expand"} ${path}`} aria-expanded={open} onClick={() => setOpen(!open)}>
      <Icon name={open ? "chevronDown" : "chevronRight"} size={12} /><span>{prefix}{array ? "[" : "{"}{!open && <><span className="json-preview"> … </span>{array ? "]" : "}"}{end}<small>{entries.length} {array ? "items" : "fields"}</small></>}</span>
    </button>}
    {open && <><div className="json-children">{entries.map(([key, child], index) => <JsonValue key={key} value={child} label={array ? undefined : key} path={`${path}.${key}`} depth={depth + 1} comma={index < entries.length - 1} />)}</div><div className="json-line">{array ? "]" : "}"}{end}</div></>}
  </div>;
}

function DocumentRecord({ record, number, expanded, onCopy }: {
  record: Record<string, unknown>; number: number; expanded: boolean; onCopy(): void;
}) {
  const [open, setOpen] = useState(expanded);
  const identityKey = ["_id", "name", "title"].find((key) => record[key] !== undefined && record[key] !== null);
  const value = identityKey ? record[identityKey] : undefined;
  const identity = value && typeof value === "object" && "$oid" in value
    ? String(value.$oid)
    : typeof value === "string" ? value : value === undefined ? "" : JSON.stringify(value);
  return <section className="document-record" aria-label={`Record ${number}`}>
    <div className="document-record-head">
      <button type="button" className="document-record-toggle" aria-expanded={open} aria-label={`${open ? "Collapse" : "Expand"} Record ${number}${identityKey ? `, ${identityKey}: ${identity}` : ""}`} onClick={() => setOpen(!open)}><Icon name={open ? "chevronDown" : "chevronRight"} size={14} /><span className="document-record-number">Record {number}</span>{identityKey && <code className="document-record-identity" title={`${identityKey}: ${identity}`}><span>{identityKey}: </span>{identity}</code>}</button>
      <button type="button" className="icon-button" aria-label={`Copy Record ${number} as JSON`} title="Copy document as JSON" onClick={onCopy}><Icon name="copy" size={14} /></button>
    </div>
    {open && <div className="document-record-body"><JsonValue value={record} path={`Record ${number}`} /></div>}
  </section>;
}

/** Pages documents without mounting an entire result set or every nested value. */
export function ResultDocuments({ result }: { result: QueryOutput }) {
  const [position, setPosition] = useState({ result, page: 0, revision: 0, expanded: true });
  const [notice, setNotice] = useState("");
  const scroll = useRef<HTMLDivElement>(null);
  if (position.result !== result) {
    setPosition({ result, page: 0, revision: position.revision + 1, expanded: true });
    setNotice("");
  }
  const page = position.result === result ? position.page : 0;
  const size = 25;
  const start = page * size;
  const end = Math.min(start + size, result.rows.length);
  const move = (next: number) => {
    setPosition((before) => ({ ...before, page: next })); setNotice("");
    scroll.current?.scrollTo({ top: 0 });
  };
  const copy = async (record: Record<string, unknown>, number: number) => {
    try { await navigator.clipboard.writeText(JSON.stringify(record, null, 2)); setNotice(`Record ${number} copied as JSON.`); }
    catch { setNotice("Could not copy. Select the document text and copy it manually."); }
  };
  return <div className="result-documents">
    {result.rows.length === 0 ? <div className="document-empty"><Icon name="file" size={24} /><p>No documents matched this query.</p><span>Check the selected database, collection, and filter.</span></div> : <>
      <div className="document-view-toolbar"><span>Documents</span><div><button type="button" className="btn ghost" onClick={() => setPosition((before) => ({ ...before, expanded: true, revision: before.revision + 1 }))}><Icon name="expandAll" size={13} />Expand records</button><button type="button" className="btn ghost" onClick={() => setPosition((before) => ({ ...before, expanded: false, revision: before.revision + 1 }))}><Icon name="collapseAll" size={13} />Collapse records</button></div></div>
      <div ref={scroll} className="result-documents-scroll">{result.rows.slice(start, end).map((row, index) => {
        const record = Object.fromEntries(result.columns.map((column, i) => [column, structured(row[i] ?? null)]));
        const number = start + index + 1;
        return <DocumentRecord key={`${position.revision}-${page}-${index}`} record={record} number={number} expanded={position.expanded} onCopy={() => void copy(record, number)} />;
      })}</div>
      <footer className="result-pagination"><span>{start + 1}–{end} of {result.rows.length.toLocaleString()} records</span><button type="button" className="btn ghost" disabled={page === 0} onClick={() => move(page - 1)}><Icon name="chevronLeft" size={12} />Previous</button><button type="button" className="btn ghost" disabled={end >= result.rows.length} onClick={() => move(page + 1)}>Next<Icon name="chevronRight" size={12} /></button><span role="status" className="document-copy-notice">{notice}</span></footer>
    </>}
  </div>;
}
