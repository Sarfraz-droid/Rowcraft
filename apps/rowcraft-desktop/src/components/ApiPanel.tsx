import { useEffect, useRef, useState } from "react";
import { API_PORT_KEY, DEFAULT_API_PORT, apiSnippets } from "../api-snippets";
import { Icon } from "./Icon";

function savedPort(): number {
  try {
    const port = Number(localStorage.getItem(API_PORT_KEY));
    return Number.isInteger(port) && port > 0 && port < 65536 ? port : DEFAULT_API_PORT;
  } catch { return DEFAULT_API_PORT; }
}

/** Right sidebar with ready-to-copy curl commands for the active data source and editor SQL. */
export function ApiPanel({ source, getSql, onClose, onShowAssistant, onAddSource }: {
  source: string; getSql(): string; onClose(): void; onShowAssistant(): void; onAddSource(): void;
}) {
  const [sql, setSql] = useState(getSql);
  const [port, setPort] = useState(savedPort);
  const [copied, setCopied] = useState("");
  const [failure, setFailure] = useState("");
  const latest = useRef(getSql);
  latest.current = getSql;
  // The editor does not report keystrokes upward, so follow its text while the panel is open.
  useEffect(() => {
    setSql(latest.current());
    const timer = window.setInterval(() => setSql(latest.current()), 800);
    return () => window.clearInterval(timer);
  }, [source]);
  useEffect(() => { try { localStorage.setItem(API_PORT_KEY, String(port)); } catch { /* the default port still works */ } }, [port]);
  const snippets = source ? apiSnippets({ source, sql, port }) : [];
  function copy(id: string, command: string) {
    setFailure("");
    void navigator.clipboard.writeText(command).then(() => setCopied(id)).catch(() => setFailure("Could not copy to the clipboard."));
  }
  return <aside className="ai-assistant-sidebar" aria-label="HTTP API">
    <header className="ai-assistant-header"><h2><Icon name="link" size={14} />HTTP API</h2><div className="tw:flex tw:items-center"><button type="button" className="icon-button" aria-label="Show AI assistant" title="AI assistant" onClick={onShowAssistant}><Icon name="ai" size={13} /></button><button type="button" className="icon-button" aria-label="Close HTTP API" title="Close" onClick={onClose}><Icon name="close" size={13} /></button></div></header>
    {source && <div className="ai-assistant-context"><Icon name="database" size={12} /><span title={source}>{source}</span></div>}
    <div className="ai-assistant-scroll">
      {!source ? <div className="ai-assistant-intro"><Icon name="link" size={24} /><h2>Call any connection over HTTP</h2><p>Select a data source to get curl commands for it.</p><button type="button" className="btn" onClick={onAddSource}><Icon name="plus" size={13} />Connect a data source</button></div> : <>
        <p className="api-panel-note">Start <code>rowcraft serve</code> in a terminal, then paste any command below. The SQL comes from the active console.</p>
        <label className="api-panel-port">Port<input type="number" min={1} max={65535} value={port} onChange={event => { const next = Number(event.target.value); if (Number.isInteger(next) && next > 0 && next < 65536) setPort(next); }} /></label>
        {snippets.map(snippet => <section className="api-snippet" key={snippet.id} aria-label={snippet.label}>
          <div className="api-snippet-heading"><span>{snippet.label}</span><button type="button" className="icon-button" aria-label={copied === snippet.id ? `Copied: ${snippet.label}` : `Copy: ${snippet.label}`} title={copied === snippet.id ? "Copied" : "Copy"} onClick={() => copy(snippet.id, snippet.command)}><Icon name={copied === snippet.id ? "check" : "copy"} size={12} /></button></div>
          <pre className="ai-code">{snippet.command}</pre>
        </section>)}
        {failure && <p className="tw:text-destructive tw:text-label" role="alert">{failure}</p>}
      </>}
    </div>
  </aside>;
}
