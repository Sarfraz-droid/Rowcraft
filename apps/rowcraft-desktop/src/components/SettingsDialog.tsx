import { useEffect, useRef, useState } from "react";
import { api, errorText } from "../api";
import type { AiModel, AiSettingsView, RouterStatus } from "../ai";
import { Check, Field, Note, TextArea, TextField } from "./form";
import { Icon } from "./Icon";
import { SelectField } from "./SelectField";
import { EDITOR_FONT_SIZES, type EditorPreferences } from "../preferences";
import { DownloadedDrivers } from "./DownloadedDrivers";

export type SettingsSection = "general" | "ai" | "drivers";
export function SettingsDialog({ onClose, initialSection, theme, editorPreferences, onSaveGeneral }: {
  onClose(): void;
  initialSection: SettingsSection;
  theme: "light" | "dark";
  editorPreferences: EditorPreferences;
  onSaveGeneral(theme: "light" | "dark", preferences: EditorPreferences): void;
}) {
  const [section, setSection] = useState(initialSection);
  const general = { theme, ...editorPreferences };
  const dialog = useRef<HTMLDialogElement>(null);
  const [settings, setSettings] = useState<AiSettingsView | null>(null);
  const [checking, setChecking] = useState(false);
  const [router, setRouter] = useState<RouterStatus | null>(null);
  const [key, setKey] = useState("");
  const [clearKey, setClearKey] = useState(false);
  const [fallbacks, setFallbacks] = useState("");
  const [models, setModels] = useState<AiModel[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  useEffect(() => {
    if (section !== "ai" || settings) return;
    let active = true;
    Promise.all([api.aiSettings(), api.aiRouterStatus()]).then(([settings, router]) => {
      if (active) { setSettings(settings); setFallbacks(settings.fallback_models.join("\n")); setRouter(router); }
    }).catch(error => { if (active) setError(errorText(error)); });
    return () => { active = false; };
  }, [section, !!settings]);
  useEffect(() => {
    if (section !== "ai" || !settings || busy) return;
    let active = true;
    let inFlight = false;
    const refresh = async () => {
      if (inFlight) return;
      inFlight = true; setChecking(true);
      try { const status = await api.aiRouterStatus(settings.base_url); if (active) setRouter(status); }
      catch { /* Keep the last known status while a URL is being edited. */ }
      finally { inFlight = false; if (active) setChecking(false); }
    };
    const initial = setTimeout(() => void refresh(), 500);
    const timer = setInterval(() => void refresh(), 8000);
    return () => { active = false; clearTimeout(initial); clearInterval(timer); };
  }, [section, settings?.base_url, busy]);
  const draft = settings && { base_url: settings.base_url, model: settings.model, fallback_models: fallbacks.split("\n").map(value => value.trim()).filter(Boolean) };
  async function operation(label: string, work: () => Promise<void>) {
    setBusy(label); setError(""); setMessage("");
    try { await work(); } catch (error) { setError(errorText(error)); } finally { setBusy(null); }
  }
  function updateGeneral(next: typeof general) {
    try {
      onSaveGeneral(next.theme, { fontSize: next.fontSize, lineWrapping: next.lineWrapping });
      setError(""); setMessage("General settings saved.");
    } catch (error) { setError(errorText(error)); setMessage(""); }
  }
  return <dialog ref={dialog} className="dialog settings-dialog" aria-labelledby="settings-title" onCancel={event => { event.preventDefault(); if (!busy) onClose(); }}>
    <form className="dialog-form" onSubmit={event => { event.preventDefault(); if (busy) return;
      if (section !== "ai") return;
      if (!draft) return; void operation("Saving…", async () => {
      const saved = await api.saveAiSettings(draft, key || null, clearKey);
      setSettings(saved); setKey(""); setClearKey(false); setMessage("AI settings saved.");
    }); }}>
      <header className="dialog-head"><div className="settings-header"><h2 id="settings-title">Settings</h2><button type="button" className="icon-button" aria-label="Close settings" disabled={!!busy} onClick={onClose}><Icon name="close" /></button></div></header>
      <div className="settings-layout">
        <nav className="settings-nav" aria-label="Settings sections">
          <button type="button" aria-current={section === "general" ? "page" : undefined} disabled={!!busy} onClick={() => { setSection("general"); setMessage(""); setError(""); }}><Icon name="settings" />General</button>
          <button type="button" aria-current={section === "drivers" ? "page" : undefined} disabled={!!busy} onClick={() => { setSection("drivers"); setMessage(""); setError(""); }}><Icon name="download" />Drivers</button>
          <button type="button" aria-current={section === "ai" ? "page" : undefined} disabled={!!busy} onClick={() => { setSection("ai"); setMessage(""); setError(""); }}><Icon name="ai" />AI</button>
        </nav>
        <div className="settings-content">
      <div className="dialog-body settings-body">
        {section === "drivers" && <DownloadedDrivers onBusyChange={value => setBusy(value ? "Updating drivers…" : null)} />}
        <section hidden={section !== "general"} aria-labelledby="general-settings-heading" className="general-settings">
          <div><h3 id="general-settings-heading">General</h3><Note>Personalize your workbench and SQL editor.</Note></div>
          <Field label="Theme"><SelectField aria-label="Theme" value={general.theme} onValueChange={theme => updateGeneral({ ...general, theme: theme as "light" | "dark" })} options={[{ value: "dark", label: "Dark" }, { value: "light", label: "Light" }]} /></Field>
          <Field label="Font size"><SelectField aria-label="Font size" value={String(general.fontSize)} onValueChange={fontSize => updateGeneral({ ...general, fontSize: Number(fontSize) })} options={EDITOR_FONT_SIZES.map(size => ({ value: String(size), label: `${size} px${size === 13 ? " (default)" : ""}` }))} /></Field>
          <Note>Resizes text and controls throughout the app. Use ⌘/Ctrl + or − to resize, and ⌘/Ctrl 0 to reset.</Note>
          <div><Check label="Wrap long lines in the SQL editor" checked={general.lineWrapping} onChange={lineWrapping => updateGeneral({ ...general, lineWrapping })} /><Note>Keep long statements visible without scrolling horizontally.</Note></div>
          <Note>Changes apply immediately and are saved on this device for all workspaces.</Note>
        </section>
        <section hidden={section !== "ai"} aria-labelledby="ai-settings-heading" className="ai-settings-body">
          <h3 id="ai-settings-heading">AI providers & routing</h3>
        {!settings ? <p role="status">{error ? "AI settings could not be loaded." : "Loading AI settings…"}</p> : <fieldset disabled={!!busy} className="tw:m-0 tw:min-w-0 tw:flex tw:flex-col tw:gap-[16px] tw:border-0 tw:p-0">
          <section className="tw:flex tw:flex-col tw:gap-[12px]" aria-labelledby="router-heading">
            <h3 id="router-heading" className="tw:m-0 tw:text-ui tw:font-semibold">Local 9router</h3>
            <Note>Connect free providers or sign in with OAuth in the provider dashboard. Choose a routing combo to let 9router switch providers when a route becomes unavailable.</Note>
            <div className="tw:flex tw:flex-wrap tw:items-center tw:gap-[8px]"><span className="tw:text-muted-foreground tw:text-label">{checking ? "Checking gateway…" : router?.running ? router.managed ? `Running · v${router.version}` : "Running · existing gateway" : router?.installed ? `Installed · v${router.version}` : "Not running"}</span>
              <button type="button" className="btn primary" disabled={!!busy || checking || router?.running} onClick={() => void operation("Installing and starting 9router…", async () => { setRouter(await api.startAiRouter(settings.base_url)); setMessage("9router is ready. Open provider setup to connect your accounts."); })}>Start 9router</button>
              <button type="button" className="btn" disabled={!!busy || checking || !router?.running || (!router.managed && (!router.pid || router.base_url !== settings.base_url.trim().replace(/\/+$/, "")))} onClick={() => void operation("Stopping 9router…", async () => { setRouter(await api.stopAiRouter(settings.base_url)); setMessage("9router stopped."); })}>Stop</button><button type="button" className="icon-button" aria-label="Refresh router status" disabled={!!busy || checking} onClick={() => void operation("Checking gateway…", async () => { setRouter(await api.aiRouterStatus(settings.base_url)); })}><Icon name="refresh" size={13} /></button>
              <button type="button" className="btn" disabled={!!busy} onClick={() => void operation("Opening provider setup…", async () => { await api.openAiDashboard(settings); })}>Provider & OAuth setup <Icon name="link" size={12} /></button>
            </div>
            <Note>{router?.running && !router.managed ? router.pid ? "Stop closes this local 9router, even when it was started outside this app." : "Using an existing gateway. Stop is available for a local 9router process." : "The first start installs 9router locally and requires Node.js 22+ with npm. The app stops its router when you quit."}</Note>
          </section>
          <TextField label="Gateway URL" value={settings.base_url} onChange={base_url => { setSettings({ ...settings, base_url }); setModels([]); }} placeholder="http://127.0.0.1:20128/v1" />
          <Field label="Gateway API key"><input className="input" type="password" autoComplete="new-password" value={key} onChange={event => setKey(event.target.value)} placeholder={settings.has_api_key ? "Saved in Keychain; enter a new key to replace" : "Optional for gateways that allow keyless access"} /></Field>
          <Note>The local 9router at port 20128 automatically uses an active key from its own configuration. Enter a key here to override it or connect to another gateway.</Note>
          {settings.has_api_key && <Check label="Remove saved gateway key" checked={clearKey} onChange={setClearKey} />}
          <div className="tw:flex tw:items-center tw:gap-[10px]"><button type="button" className="btn" disabled={!!busy} onClick={() => void operation("Checking gateway…", async () => { const models = await api.aiModels(settings, key || null); setModels(models); setMessage(`Connected · ${models.length} models and routes available.`); })}>Test & load routes</button><span className="tw:text-caption tw:text-muted-foreground">Gateway keys are saved in the OS credential store.</span></div>
          {!!models.length && <Field label="Available models & routes"><SelectField aria-label="Available models & routes" value={settings.model} onValueChange={model => setSettings({ ...settings, model })} placeholder="Choose a model or routing combo" options={models.map(model => ({ value: model.id, label: model.owned_by === "combo" ? `${model.id} · routing combo` : model.id }))} /></Field>}
          <TextField label="Model or routing combo ID" value={settings.model} onChange={model => setSettings({ ...settings, model })} placeholder="Select a route above or enter its ID" />
          <TextArea label="Fallback model IDs (optional, one per line)" value={fallbacks} onChange={setFallbacks} rows={3} placeholder="Additional routes, in priority order" />
          <Note>Fallbacks run in order on rate limits, missing models, and server errors. A 9router combo also manages provider quotas and OAuth refresh automatically.</Note>
        </fieldset>}
        </section>
        {error && <p className="tw:m-0 tw:text-destructive tw:whitespace-pre-wrap" role="alert">{error}</p>}
        <p className="tw:m-0 tw:text-muted-foreground tw:text-label" role="status" aria-live="polite">{busy || message}</p>
      </div>
      <footer className="dialog-foot"><span className="grow" /><button type="button" className="btn" disabled={!!busy} onClick={onClose}>Close</button>{section === "ai" && <button type="submit" className="btn primary" disabled={!!busy || !settings}>Save settings</button>}</footer>
        </div>
      </div>
    </form>
  </dialog>;
}
