import { useEffect, useRef, useState } from "react";
import { api, errorText, type Catalog } from "../api";
import { databaseProfile } from "../database-profile";
import { suggestedSql, type SqlAction } from "../ai";
import { runSqlAssistance, type ToolTrace } from "../ai-harness";
import { Check } from "./form";
import { Icon } from "./Icon";

export interface InlineTarget { tabId: string; from: number; to: number; fullSql: string; selectedSql: string; left: number; top: number }

export function InlineSqlAssistant({ target, dialect, source, schema, catalog, error, onClose, onSettings, onApply }: {
  target: InlineTarget; dialect: string; source: string; schema: string; catalog: Catalog | null; error?: string;
  onClose(): void; onSettings(): void; onApply(sql: string, action: SqlAction): boolean;
}) {
  const profile = databaseProfile(dialect);
  const [prompt, setPrompt] = useState("");
  const action = target.selectedSql.trim() || (error && target.fullSql.trim()) ? "fix" : "generate";
  const [optionsOpen, setOptionsOpen] = useState(false);
  const [query, setQuery] = useState<string | null>(null);
  const [failure, setFailure] = useState("");
  const [busy, setBusy] = useState(false);
  const [allowTools, setAllowTools] = useState(true);
  const [traces, setTraces] = useState<ToolTrace[]>([]);
  const request = useRef<AbortController | null>(null);
  useEffect(() => () => request.current?.abort(), []);
  async function ask() {
    if (busy || (!prompt.trim() && !(action === "fix" && error))) return;
    const controller = new AbortController(); request.current = controller;
    setBusy(true); setFailure(""); setQuery(null); setTraces([]);
    try {
      const settings = await api.aiSettings();
      if (controller.signal.aborted) return;
      if (!settings.model) throw new Error("Choose a model in AI settings first.");
      const reply = await runSqlAssistance({ action, prompt, sql: target.selectedSql || target.fullSql, source, dialect, schema, catalog, includeSchema: false, allowTools, error }, setTraces, controller.signal);
      if (controller.signal.aborted) return;
      const query = suggestedSql(reply.content);
      if (!query) throw new Error(reply.content || "The model returned no SQL. Try a more specific instruction.");
      setQuery(query);
    } catch (error) { if (!controller.signal.aborted) setFailure(errorText(error)); }
    finally { if (!controller.signal.aborted) setBusy(false); }
  }
  return <section className="inline-ai" aria-label="Inline AI edit" style={{ left: target.left, top: target.top }} onKeyDown={event => { if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); onClose(); } }}>
    <form onSubmit={event => { event.preventDefault(); void ask(); }}>
      <div className="inline-ai-prompt">
        <Icon name="ai" size={14} />
        <textarea autoFocus aria-label="Inline SQL instruction" placeholder={`Ask AI to write or edit ${profile.language}…`} value={prompt} disabled={busy} onChange={event => { setPrompt(event.target.value); setQuery(null); }} rows={1} onKeyDown={event => {
          if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
            event.preventDefault(); event.stopPropagation(); void ask();
          }
        }} />
        <button type="button" className="icon-button" aria-label="Inline AI options" aria-expanded={optionsOpen} onClick={() => setOptionsOpen(open => !open)}><Icon name="filter" size={12} /></button>
        <button type="button" className="icon-button" aria-label="Open AI settings" title="AI settings · Start 9router or configure providers" onClick={onSettings}><Icon name="settings" size={12} /></button>
        <button type="submit" className="icon-button" aria-label="Submit inline AI instruction" title="Submit (Enter)" disabled={busy || (!prompt.trim() && !(action === "fix" && error))}>{busy ? <span className="spinner" aria-hidden /> : <Icon name="arrow" size={14} />}</button>
        <button type="button" className="icon-button" aria-label="Close inline AI" title="Dismiss (Esc)" onClick={onClose}><Icon name="close" size={12} /></button>
      </div>
      {optionsOpen && <div className="inline-ai-options"><Check label="Allow read-only database tools" checked={allowTools} onChange={setAllowTools} /><button type="button" className="btn ghost" onClick={onSettings}>AI settings</button></div>}
      {traces.length > 0 && <details className="ai-tool-log"><summary>{traces.length} database tool calls</summary>{traces.map((trace, index) => <pre key={`${trace.id}-${index}`} className="ai-code">{trace.sql || trace.name}: {trace.status}{trace.result ? `\n${trace.result}` : ""}</pre>)}</details>}
      {failure && <div><p role="alert" className="tw:m-0 tw:text-label tw:text-destructive tw:whitespace-pre-wrap">{failure}</p><button type="button" className="btn ghost" onClick={onSettings}>AI settings</button></div>}
      {query && <pre className="ai-code inline-ai-preview" aria-label="SQL edit preview">{query}</pre>}
      {query && <footer><small>Review before applying · Esc dismiss</small><button type="button" className="btn" onClick={onClose}>Discard</button><button type="button" className="btn primary" onClick={() => { if (!onApply(query, action)) setFailure("The SQL changed while AI was working. Reopen inline AI to apply an edit to the latest script."); }}>Apply edit</button></footer>}
    </form>
  </section>;
}
