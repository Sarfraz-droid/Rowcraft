import { databaseProfile } from "../database-profile";
import ReactMarkdown from "react-markdown";
import { Children, isValidElement, useEffect, useRef, useState, type ReactNode } from "react";
import { api, errorText, type Catalog } from "../api";
import { suggestedSql, type AiModel, type AiReply } from "../ai";
import { runSqlAssistance, type ToolTrace } from "../ai-harness";
import { SelectField } from "./SelectField";
import { Check, Note } from "./form";
import { Icon } from "./Icon";

export function SqlAssistant({ sql, getSql, source = "", title = "SQL console", dialect, schema, catalog, error, enabled = true, settingsVersion = 0, onClose, onSettings, onUse, onOpenConsole }: {
  sql: string; getSql?(): string; source?: string; title?: string; dialect: string; schema: string; catalog: Catalog | null; error?: string; enabled?: boolean; settingsVersion?: number;
  onClose(): void; onSettings(): void; onUse(sql: string): void; onOpenConsole?(): void;
}) {
  const profile = databaseProfile(dialect);
  const [prompt, setPrompt] = useState("");
  const [includeSchema, setIncludeSchema] = useState(false);
  const [allowTools, setAllowTools] = useState(true);
  const [turns, setTurns] = useState<{ prompt: string; reply: AiReply }[]>([]);
  const [pending, setPending] = useState("");
  const [failure, setFailure] = useState("");
  const [busy, setBusy] = useState(false);
  const [configured, setConfigured] = useState<boolean | null>(null);
  const [route, setRoute] = useState("");
  const [models, setModels] = useState<AiModel[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelsError, setModelsError] = useState("");
  const [copied, setCopied] = useState<number | null>(null);
  const [traces, setTraces] = useState<ToolTrace[]>([]);
  const request = useRef<AbortController | null>(null);
  const scroll = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (!input.current) return;
    input.current.style.height = "auto";
    input.current.style.height = `${Math.min(144, Math.max(64, input.current.scrollHeight))}px`;
  }, [prompt]);
  useEffect(() => {
    let active = true;
    setBusy(false); setPending(""); setModels([]); setModelsError(""); setModelsLoading(true);
    api.aiSettings().then(async settings => {
      if (!active) return;
      setConfigured(!!settings.model); setRoute(settings.model);
      try {
        const available = await api.aiModels(settings, null);
        if (active) setModels(available);
      } catch (error) { if (active) setModelsError(errorText(error)); }
    }).catch(error => { if (active) { setFailure(errorText(error)); setConfigured(false); } }).finally(() => { if (active) setModelsLoading(false); });
    return () => { active = false; request.current?.abort(); };
  }, [settingsVersion]);
  useEffect(() => { scroll.current?.scrollTo?.({ top: scroll.current.scrollHeight, behavior: "smooth" }); }, [turns, pending, traces, busy, failure]);
  const canAsk = enabled && configured !== null && !!route && !busy && !!prompt.trim();
  const modelOptions = [...new Set([route, ...models.map(model => model.id)].filter(Boolean))].map(id => ({ value: id, label: models.find(model => model.id === id)?.owned_by === "combo" ? `${id} · routing combo` : id }));
  const toolsRunning = traces.some(trace => trace.status === "running");
  const toolsFailed = traces.some(trace => trace.status === "error");
  const starters = [
    { icon: "table" as const, label: `Explore my ${profile.objects.toLowerCase()}`, prompt: `Show me the ${profile.objects.toLowerCase()} in the selected ${profile.namespace.toLowerCase()} and briefly explain what is available.` },
    ...(sql.trim() ? [{ icon: "console" as const, label: `Explain this ${profile.language}`, prompt: `Explain the ${profile.language} in the current editor.` }] : []),
    { icon: "ai" as const, label: error ? "Help fix this error" : "Write a query", prompt: error ? `Help me fix the current ${profile.language} and database error.` : `Write a ${profile.language} query to ` },
  ];
  function stop() {
    request.current?.abort(); setBusy(false); setPending("");
    setTraces(previous => previous.map(trace => trace.status === "running" ? { ...trace, status: "error", result: "Request stopped." } : trace));
  }
  async function ask() {
    if (!canAsk) return;
    const controller = new AbortController(); request.current = controller;
    const submitted = prompt.trim();
    setBusy(true); setFailure(""); setPending(submitted); setCopied(null); setTraces([]);
    try { const result = await runSqlAssistance({ action: "chat", model: route, prompt: submitted, history: turns.flatMap(turn => [{ role: "user" as const, content: turn.prompt }, { role: "assistant" as const, content: turn.reply.content }]), sql: getSql?.() ?? sql, source, dialect, schema, catalog, includeSchema, allowTools: allowTools && !!source, error }, setTraces, controller.signal); if (!controller.signal.aborted) { setTurns(previous => [...previous, { prompt: submitted, reply: result }]); setPrompt(""); } }
    catch (error) { if (!controller.signal.aborted) setFailure(errorText(error)); }
    finally { if (!controller.signal.aborted) { setBusy(false); setPending(""); } }
  }
  return <aside className="ai-assistant-sidebar" aria-label="AI assistant">
    <header className="ai-assistant-header"><h2><Icon name="ai" size={14} />AI assistant</h2><div className="tw:flex tw:items-center">{turns.length > 0 && <button type="button" className="icon-button" aria-label="New conversation" title="New conversation" disabled={busy} onClick={() => { setTurns([]); setTraces([]); setFailure(""); setPrompt(""); input.current?.focus(); }}><Icon name="plus" size={13} /></button>}<button type="button" className="icon-button" aria-label="Close SQL assistant" title="Close assistant" onClick={onClose}><Icon name="close" size={13} /></button></div></header>
    {enabled && <div className="ai-assistant-context"><Icon name="console" size={12} /><span title={`${source} · ${title}`}>{source ? `${source} / ` : ""}{title}</span>{schema && <small title={`Schema: ${schema}`}>{schema}</small>}</div>}
    <div className="ai-assistant-scroll" ref={scroll}>
      {turns.length === 0 && !busy && <div className="ai-assistant-intro"><Icon name="ai" size={24} /><h2>{enabled ? "What are you working on?" : `Start with a ${profile.console}`}</h2><p>{enabled ? "Write a query, understand your data, or work through an error." : "Open a console to work with AI using your SQL and database context."}</p>
        {enabled ? <div className="ai-prompt-starters" aria-label="Suggested prompts">{starters.map(starter => <button type="button" key={starter.label} onClick={() => { setPrompt(starter.prompt); input.current?.focus(); }}><Icon name={starter.icon} size={14} /><span>{starter.label}</span><Icon name="arrow" size={12} /></button>)}</div> : onOpenConsole && <button type="button" className="btn" onClick={onOpenConsole}><Icon name={source ? "console" : "plus"} size={13} />{source ? `Open ${profile.console}` : "Connect a data source"}</button>}
      </div>}
      {turns.map((turn, index) => {
        return <div className="ai-chat-turn" key={index}>
          <p className="ai-chat-prompt">{turn.prompt}</p>
          <section aria-label="AI response"><div className="ai-message-heading"><Icon name="ai" size={12} /><span>Assistant</span><small title={turn.reply.model}>{turn.reply.model}</small></div><div className="ai-markdown"><ReactMarkdown components={{ pre: ({ children }) => {
            const code = Children.toArray(children).find(child => isValidElement(child));
            const props = isValidElement<{ className?: string; children?: ReactNode }>(code) ? code.props : null;
            const language = props?.className?.replace(/^language-/, "") ?? "";
            const text = Children.toArray(props?.children).filter(child => typeof child === "string").join("");
            const generated = suggestedSql(["```" + language, text, "```"].join("\n"));
            return <div className="ai-code-block"><pre>{children}</pre>{generated && <div className="ai-response-actions"><button type="button" className="btn" disabled={!enabled} onClick={() => onUse(generated)}><Icon name="console" size={12} />Use {profile.language} in editor</button></div>}</div>;
          } }}>{turn.reply.content}</ReactMarkdown></div><div className="ai-response-actions"><button type="button" className="icon-button" aria-label={copied === index ? "Copied response" : "Copy response"} title={copied === index ? "Copied" : "Copy response"} onClick={() => void navigator.clipboard.writeText(turn.reply.content).then(() => setCopied(index)).catch(error => setFailure(errorText(error)))}><Icon name={copied === index ? "check" : "copy"} size={12} /></button></div></section>
        </div>;
      })}
      {pending && <p className="ai-chat-prompt">{pending}</p>}
      {configured === false && !route && <div className="tw:flex tw:flex-col tw:items-start tw:gap-[10px]"><Note>Choose an AI model or routing combo in settings to use SQL assistance.</Note><button type="button" className="btn" onClick={onSettings}>Configure AI providers</button></div>}
      {traces.length > 0 && <details className="ai-tool-log ai-tool-activity">
        <summary><Icon name="chevronRight" size={12} className="ai-tool-chevron" /><span>Database activity</span><small role="status">{toolsRunning ? "Running…" : toolsFailed ? "Completed with errors" : "Done"}</small>{toolsRunning ? <span className="spinner" aria-hidden="true" /> : <Icon name={toolsFailed ? "close" : "check"} size={12} />}</summary>
        <section aria-label="Database tool activity">{traces.map((trace, index) => <details key={`${trace.id}-${index}`}><summary>{trace.status === "running" ? <span className="spinner" aria-hidden="true" /> : <Icon name={trace.status === "error" ? "close" : "check"} size={11} />}{trace.name === "read_schema" ? "Inspect schema" : "Read-only query"}<small>{trace.status === "running" ? "Running…" : trace.status === "error" ? "Failed" : "Done"}</small></summary>{trace.sql && <pre className="ai-code">{trace.sql}</pre>}{trace.result && <pre className="ai-code">{trace.result}</pre>}</details>)}</section>
      </details>}
      {failure && <div className="ai-assistant-error"><p className="tw:text-destructive tw:text-label tw:whitespace-pre-wrap" role="alert">{failure}</p><button type="button" className="btn ghost" onClick={onSettings}>Open AI settings<Icon name="arrow" size={12} /></button></div>}
      {busy && <p role="status" className="tw:flex tw:items-center tw:gap-[8px] tw:text-label tw:text-muted-foreground"><span className="spinner" />{toolsRunning ? "Inspecting database…" : "Thinking…"}</p>}
    </div>
    <form className="ai-assistant-composer" onSubmit={event => { event.preventDefault(); void ask(); }}>
      {modelsError && <div className="ai-model-warning"><span>Could not load models.</span><button type="button" className="text-button" aria-label="Open AI settings" onClick={onSettings}>Check settings</button></div>}
      <div className="ai-composer-box">
        <textarea ref={input} aria-label="Message AI assistant" value={prompt} disabled={busy || !enabled} onChange={event => setPrompt(event.target.value)} rows={2} placeholder={enabled ? `Ask about your ${profile.language} or database…` : "Open a console to start a conversation"} onKeyDown={event => { if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); void ask(); } }} />
        <div className="ai-composer-footer"><SelectField className="ai-model-select tw:border-transparent tw:bg-transparent tw:px-[4px] tw:text-meta tw:text-muted-foreground" aria-label="Assistant model" title={route || "Choose a model"} value={route} onValueChange={setRoute} options={modelOptions} disabled={busy || modelsLoading || !modelOptions.length} placeholder={modelsLoading ? "Loading models…" : "Choose model"} />{busy ? <button type="button" className="btn ai-send-button" aria-label="Stop response" onClick={stop}><Icon name="close" size={13} />Stop</button> : <button type="submit" className="btn primary ai-send-button" disabled={!canAsk} title="Send message (Enter)">Send<Icon name="arrow" size={13} /></button>}</div>
      </div>
      <div className="ai-composer-meta"><details className="ai-context-options"><summary><Icon name="filter" size={12} />Context & tools<Icon name="chevronDown" size={10} /></summary><div><p className="ai-route-label" title={enabled ? `${title} · ${dialect.toUpperCase()}` : undefined}>{enabled ? `${title}${schema ? ` · ${schema}` : ""}` : "No SQL console selected"}</p><Check label="Allow read-only database tools" checked={allowTools} onChange={setAllowTools} /><Check label="Include selected schema structure" checked={includeSchema} onChange={setIncludeSchema} /><Note>{allowTools ? "Tool results and schema metadata may be sent to your AI provider. Queries return at most 25 rows." : "Database tools are off. Only your SQL, messages, and selected context are sent."}</Note></div></details><small>⇧Enter for newline</small></div>

    </form>
  </aside>;
}
