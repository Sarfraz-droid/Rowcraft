import { useEffect, useState } from "react";
import { api, type McpStatus } from "../api";
import { errorText } from "../errors";
import { Icon } from "./Icon";

/** Connect AI clients (Claude Code, Codex, …) to Rowcraft's MCP server without a terminal. */
export function McpConnect() {
  const [allowWrite, setAllowWrite] = useState(false);
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [busy, setBusy] = useState("");
  const [failure, setFailure] = useState("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let active = true;
    setFailure("");
    api.mcpStatus(allowWrite).then(next => { if (active) setStatus(next); }).catch(e => { if (active) setFailure(errorText(e)); });
    return () => { active = false; };
  }, [allowWrite]);

  async function change(id: string, connect: boolean) {
    setBusy(id);
    setFailure("");
    try { setStatus(await (connect ? api.mcpConnect(id, allowWrite) : api.mcpDisconnect(id, allowWrite))); }
    catch (e) { setFailure(errorText(e)); }
    finally { setBusy(""); }
  }
  function copy() {
    if (!status) return;
    void navigator.clipboard.writeText(status.config_json).then(() => setCopied(true)).catch(() => setFailure("Could not copy to the clipboard."));
  }

  return <section className="api-snippet" aria-label="AI clients (MCP)">
    <div className="api-snippet-heading"><span>AI clients (MCP)</span></div>
    <p className="api-panel-note">Let Claude Code, Codex and other AI clients list, inspect and query your saved data sources.</p>
    <label className="api-panel-port"><input type="checkbox" checked={allowWrite} onChange={event => setAllowWrite(event.target.checked)} /> Allow changes (INSERT, UPDATE, DELETE, DDL)</label>
    <p className="api-panel-note">{allowWrite ? "Connect again after changing this: clients keep the setting they were connected with." : "Read-only: only SELECT-style statements run."}</p>
    {status?.clients.map(client => <div className="api-snippet-heading" key={client.id}>
      <span>{client.label}{!client.installed ? " · not found" : client.registered ? " · connected" : ""}</span>
      <button type="button" className="btn" disabled={!client.installed || !!busy} onClick={() => void change(client.id, true)}>{busy === client.id ? "Working…" : client.registered ? "Reconnect" : "Connect"}</button>
      {client.registered && <button type="button" className="btn" disabled={!!busy} onClick={() => void change(client.id, false)}>Disconnect</button>}
    </div>)}
    {status && <details>
      <summary className="api-panel-note">Other clients (Cursor, Claude Desktop, …)</summary>
      <div className="api-snippet-heading"><span>mcpServers</span><button type="button" className="icon-button" aria-label={copied ? "Copied: MCP configuration" : "Copy: MCP configuration"} title={copied ? "Copied" : "Copy"} onClick={copy}><Icon name={copied ? "check" : "copy"} size={12} /></button></div>
      <pre className="ai-code">{status.config_json}</pre>
    </details>}
    {failure && <p className="tw:text-destructive tw:text-label" role="alert">{failure}</p>}
  </section>;
}
