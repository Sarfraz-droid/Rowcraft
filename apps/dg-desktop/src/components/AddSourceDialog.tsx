import { ErrorNotice } from "./ErrorNotice";
import { SelectField } from "./SelectField";
// DataGrip's "Add data source" dialog: General, Options, SSH/SSL, Schemas and Advanced tabs over one
// `Draft`. Every rule (what is required, what is valid) lives in Rust; this only collects input and
// shows the message that comes back.

import { type FormEvent, useEffect, useMemo, useRef, useState } from "react";

import { type AddedSource, type Draft, type DriverInfo, type DriverSettings, type CustomDriverConfig, api, errorText } from "../api";
import { DriverEditor } from "./DriverEditor";
import { Check, Choice, Field, Heading, Note, TextArea, TextField } from "./form";
import { connectionUrl, parseConnectionUrl } from "../connection-url";

type Tab = "general" | "options" | "ssh-ssl" | "schemas" | "advanced";

const TABS: ReadonlyArray<readonly [string, Tab]> = [
  ["General", "general"],
  ["Options", "options"],
  ["SSH/SSL", "ssh-ssl"],
  ["Schemas", "schemas"],
  ["Advanced", "advanced"],
];

type Status = { kind: "none" | "info" | "error"; text: string };
type TextKey = { [K in keyof Draft]-?: Draft[K] extends string ? K : never }[keyof Draft];

interface Props {
  sourceName?: string;
  initialDriver?: string;
  initialMode?: "sources" | "drivers";
  onClose(): void;
  onAdded(added: AddedSource): void;
}

export function AddSourceDialog({ onClose, onAdded, initialDriver, initialMode = "sources", sourceName }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const sqliteFileInput = useRef<HTMLInputElement>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [sshNames, setSshNames] = useState<string[]>([]);
  const [drivers, setDrivers] = useState<DriverInfo[]>([]);
  const [mode, setMode] = useState<"sources" | "drivers">(initialMode);
  const [settings, setSettings] = useState<DriverSettings>({ custom: [], dialects: [] });
  const [editedDriver, setEditedDriver] = useState<CustomDriverConfig | null>(null);
  const [driverBusy, setDriverBusy] = useState(false);
  const [search, setSearch] = useState("");
  const [tab, setTab] = useState<Tab>("general");
  const [status, setStatus] = useState<Status>({ kind: "none", text: "" });
  const [busy, setBusy] = useState<"test" | "save" | "upload" | "ssh" | null>(null);
  const [customUrl, setCustomUrl] = useState(false);
  const [savedPasswords, setSavedPasswords] = useState({ password: false, key_password: false });

  useEffect(() => {
    dialog.current?.showModal();
    let active = true;
    Promise.all([sourceName ? api.editSource(sourceName) : api.formDefaults(), api.drivers(), api.driverSettings()])
      .then(async ([defaults, catalog, driverSettings]) => {
        if (!active) return;
        setSettings(driverSettings);
        const initial = initialDriver ? await api.selectDriver(defaults.draft, initialDriver) : defaults.draft;
        if (!active) return;
        setDraft(initial.dialect === "sqlite" ? { ...initial, auth: "no-auth" } : initial);
        setCustomUrl(!!initial.url);
        if ("has_saved_password" in defaults && "has_saved_key_password" in defaults) setSavedPasswords({ password: !!defaults.has_saved_password, key_password: !!defaults.has_saved_key_password });
        if (initialMode === "drivers") {
          setEditedDriver({ id: "", name: "", dialect: "jdbc", port: null, target: "network", jdbc_class: null, jdbc_jars: [], jdbc_url_template: null });
        }
        setSshNames(defaults.ssh_names);
        setDrivers(catalog);
      })
      .catch((e: unknown) => { if (active) setStatus({ kind: "error", text: errorText(e) }); });
    return () => { active = false; };
  }, []);

  const shown = useMemo(() => {
    const q = search.trim().toLowerCase();
    return drivers.filter((d) => d.name.toLowerCase().includes(q));
  }, [drivers, search]);

  if (!draft) {
    return (
      <dialog ref={dialog} className="dialog" aria-label={sourceName ? "Connection properties" : "Data Sources and Drivers"} onClose={onClose}>
        {status.kind === "error" ? <ErrorNotice error={status.text} title="Could not load data source settings" /> : <p className="dialog-loading tw:p-[24px]">Loading…</p>}
        <button type="button" className="btn" onClick={onClose}>Close</button>
      </dialog>
    );
  }

  const set = <K extends keyof Draft>(key: K, value: Draft[K]) =>
    setDraft((d) => (d ? { ...d, [key]: value } : d));
  const text = (key: TextKey, label: string, placeholder?: string) => (
    <TextField label={label} value={draft[key]} placeholder={placeholder} onChange={(v) => set(key, v)} />
  );
  const passwordField = (key: "password" | "ssh_password" | "ssh_passphrase" | "key_password", label: string) => (
    <><Field label={label}><input className="input" type="password" value={draft[key] ?? ""} placeholder={sourceName && (key === "password" || key === "key_password") && savedPasswords[key] ? "Saved · leave blank to keep" : undefined} autoComplete="new-password" onChange={(event) => setDraft((before) => before ? { ...before, [key]: event.target.value, [`${key}_env`]: "", ...(key === "password" || key === "key_password" ? { [`clear_${key}`]: false } : {}) } : before)} /></Field>{sourceName && (key === "password" || key === "key_password") && savedPasswords[key] && <Check label={`Remove saved ${key === "password" ? "password" : "client key password"}`} checked={!!draft[`clear_${key}`]} onChange={checked => setDraft(before => before ? { ...before, [`clear_${key}`]: checked, [key]: checked ? "" : before[key] } : before)} />}</>
  );
  const area = (key: TextKey, label: string, placeholder: string, rows: number) => (
    <TextArea label={label} value={draft[key]} placeholder={placeholder} rows={rows} onChange={(v) => set(key, v)} />
  );

  const current = drivers.find((d) => d.id === draft.driver);
  const sqlite = current?.dialect === "sqlite";
  const generatedUrl = connectionUrl(draft, current);
  const portHint = current?.port ? String(current.port) : "driver default";

  const uploadSqlite = async (file: File) => {
    setBusy("upload");
    setStatus({ kind: "info", text: `Importing ${file.name}…` });
    try {
      const imported = await api.importSqlite(file);
      setDraft((d) => d ? {
        ...d, database: imported.path, url: "", auth: "no-auth", user: "", password_env: "", password: "",
        name: d.name.trim() ? d.name : file.name.replace(/\.(db|db3|sqlite|sqlite3|s3db)$/i, ""),
      } : d);
      setStatus({ kind: "info", text: `${file.name} imported. Test Connection or press OK to save.` });
    } catch (e) {
      setStatus({ kind: "error", text: errorText(e) });
    } finally {
      setBusy(null);
    }
  };

  const chooseDriver = async (id: string) => {
    if (busy !== null) return;
    try {
      const selected = await api.selectDriver(draft, id);
      setDraft(selected.dialect === "sqlite" ? { ...selected, auth: "no-auth", user: "", password_env: "", password: "" } : selected);
      setStatus({ kind: "none", text: "" });
    } catch (e) {
      setStatus({ kind: "error", text: errorText(e) });
    }
  };

  const saveSshConfig = async () => {
    if (busy !== null) return;
    setBusy("ssh");
    try {
      const names = await api.saveSshConfig(draft);
      setSshNames(names);
      const name = draft.ssh_new_name.trim();
      setDraft((before) => before ? { ...before, ssh_new: false, ssh_existing: name, ssh_password: "", ssh_passphrase: "", ssh_password_env: "", ssh_passphrase_env: "" } : before);
      setStatus({ kind: "info", text: `SSH configuration “${name}” saved. It is available to all data sources.` });
    } catch (error) { setStatus({ kind: "error", text: errorText(error) }); }
    finally { setBusy(null); }
  };

  const testConnection = async () => {
    setBusy("test");
    setStatus({ kind: "info", text: "Connecting…" });
    try {
      if (sourceName) await api.testConnection(draft, sourceName); else await api.testConnection(draft);
      setStatus({ kind: "info", text: "Succeeded" });
    } catch (e) {
      setStatus({ kind: "error", text: `Failed: ${errorText(e)}` });
    } finally {
      setBusy(null);
    }
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy("save");
    try {
      onAdded(sourceName ? await api.updateSource(sourceName, draft) : await api.addSource(draft));
    } catch (e) {
      setStatus({ kind: "error", text: errorText(e) });
      setBusy(null);
    }
  };

  const editDriver = (driver: DriverInfo) => {
    if (driverBusy) return;
    setEditedDriver(settings.custom.find((d) => d.id === driver.id) ?? {
      id: `${driver.id}_custom`, name: `${driver.name} (custom)`, dialect: driver.jdbc_url_template ? "jdbc" : (driver.dialect ?? "jdbc"),
      port: driver.port, target: driver.file ? "file" : "network", jdbc_class: null, jdbc_jars: [], jdbc_url_template: driver.jdbc_url_template ?? null,
    });
  };
  const newDriver = () => {
    if (driverBusy) return;
    setMode("drivers");
    setEditedDriver({ id: "", name: "", dialect: "jdbc", port: null, target: "network", jdbc_class: null, jdbc_jars: [], jdbc_url_template: null });
    setStatus({ kind: "none", text: "" });
  };
  const saveDriver = async () => {
    if (!editedDriver || driverBusy) return;
    setDriverBusy(true);
    try {
      const updated = await api.saveDriver({ ...editedDriver, port: editedDriver.target === "file" ? null : editedDriver.port, jdbc_jars: editedDriver.jdbc_jars.map((p) => p.trim()).filter(Boolean) });
      setDrivers(updated);
      setSettings(await api.driverSettings());
      setStatus({ kind: "info", text: "Driver saved." });
    } catch (e) { setStatus({ kind: "error", text: errorText(e) }); }
    finally { setDriverBusy(false); }
  };
  const uploadDriverJars = async (files: File[]) => {
    if (!editedDriver || driverBusy) return;
    setDriverBusy(true);
    let imported = 0;
    try {
      for (const file of files) {
        setStatus({ kind: "info", text: `Uploading ${file.name}…` });
        const path = await api.importJdbcJar(file);
        setEditedDriver((before) => before ? {
          ...before, jdbc_jars: [...new Set([...before.jdbc_jars.map(p => p.trim()).filter(Boolean), path])],
        } : before);
        imported++;
      }
      setStatus({ kind: "info", text: `${imported} JAR ${imported === 1 ? "file" : "files"} uploaded. Save Driver to keep this configuration.` });
    } catch (e) {
      setStatus({ kind: "error", text: `${errorText(e)}${imported ? ` ${imported} uploaded ${imported === 1 ? "file remains" : "files remain"} in the driver files list.` : ""}` });
    } finally { setDriverBusy(false); }
  };
  const removeDriver = async () => {
    if (!editedDriver) return;
    setDriverBusy(true);
    try {
      const updated = await api.removeDriver(editedDriver.id);
      setDrivers(updated);
      setSettings(await api.driverSettings());
      setEditedDriver(null);
      if (draft.driver === editedDriver.id) setDraft(await api.selectDriver(draft, "postgresql"));
      setStatus({ kind: "info", text: "Driver removed." });
    } catch (e) { setStatus({ kind: "error", text: errorText(e) }); }
    finally { setDriverBusy(false); }
  };

  const general = (
    <>
      <div className="selected-driver"><span>Driver: <strong>{current?.name ?? "Select a database"}</strong></span><button type="button" className="text-button" onClick={() => { setMode("drivers"); if (current) editDriver(current); }}>Configure driver…</button></div>
      {current?.caveat && <Note>{current.caveat}</Note>}
      {text("name", "Name", "my-database")}
      {sqlite && <div className="field sqlite-file-field">
        <label className="field-label" htmlFor="sqlite-database-path">Database file</label>
        <div className="row sqlite-file-row">
          <input id="sqlite-database-path" className="input grow" value={draft.database} placeholder="Choose a SQLite file or enter its local path" spellCheck={false} disabled={busy !== null} onChange={(e) => set("database", e.target.value)} />
          <button type="button" className="btn" disabled={busy !== null} onClick={() => sqliteFileInput.current?.click()}>
            {busy === "upload" ? "Importing…" : "Upload SQLite file…"}
          </button>
        </div>
        <input ref={sqliteFileInput} type="file" accept=".db,.db3,.sqlite,.sqlite3,.s3db,application/vnd.sqlite3,application/x-sqlite3" hidden aria-label="Upload SQLite database" onChange={(e) => {
          const file = e.currentTarget.files?.[0];
          e.currentTarget.value = "";
          if (file) void uploadSqlite(file);
        }} />
        <Note>Upload an existing SQLite database (up to 256 MB). Imported copies are stored in the app’s database folder.</Note>
      </div>}
      {text("comment", "Comment")}
      <div className="field">
        <label className="field-label" htmlFor="source-color">Color</label>
        <div className="row center source-color-control">
          <input id="source-color" type="color" className="input source-color-picker tw:size-[32px] tw:shrink-0 tw:cursor-pointer tw:p-[3px]" value={draft.color || "#2563eb"} onChange={(event) => set("color", event.target.value)} />
          <span className="note color-value">{draft.color || "Default"}</span>
          <button type="button" className="text-button tw:whitespace-nowrap tw:shrink-0" disabled={!draft.color} onClick={() => set("color", "")}>Clear color</button>
        </div>
      </div>
      <Field label="Connection type"><SelectField aria-label="Connection type" value={draft.connection_type} onValueChange={value => set("connection_type", value)} options={[{ value: "", label: "Default" }, { value: "development", label: "Development" }, { value: "test", label: "Test" }, { value: "staging", label: "Staging" }, { value: "production", label: "Production" }]} /></Field>
      {!current?.file && (
        <div className="row">
          <div className="grow">{text("host", "Host", "localhost")}</div>
          <div className="fixed">{text("port", "Port", portHint)}</div>
        </div>
      )}
      {!sqlite && <Choice
        label="Authentication"
        value={draft.auth}
        onChange={(v) => set("auth", v)}
        options={[
          ["User & Password", "user-password"],
          ["No auth", "no-auth"],
          ["Windows credentials", "windows-credentials"],
        ]}
      />}
      {!sqlite && text("user", "User")}
      {!sqlite && draft.auth === "user-password" && passwordField("password", "Password")}
      {draft.dialect === "graphql" && <Note>Use Edit URL to enter the full GraphQL endpoint. For a bearer token, leave User empty and put the token in Password. Query documents and JSON requests with variables are supported; subscriptions are not.</Note>}
      {draft.dialect === "neo4j" && <Note>Connect through Neo4j Query API v2 (HTTP 7474, HTTPS 7473, or your Aura HTTPS endpoint). Use Edit URL for a full endpoint; Bolt URLs are not supported. Enter your database name, user, and password above.</Note>}

      {!sqlite && text("database", current?.file ? "Database file path" : "Database")}
      <div className="field">
        <label className="field-label" htmlFor="connection-url">URL</label>
        <input id="connection-url" className="input" readOnly={!customUrl} value={customUrl ? draft.url : generatedUrl} spellCheck={false} onChange={(event) => { const url = event.target.value; setDraft(before => before ? { ...before, url, ...parseConnectionUrl(url, before.properties) } : before); }} aria-describedby="connection-url-help" />
        <div className="row center source-url-controls"><span id="connection-url-help" className="note">{customUrl ? "Recognized URL values fill the fields above. URL options are copied to Advanced." : "Generated from the connection fields."}</span><button type="button" className="text-button" onClick={() => { if (customUrl) { set("url", ""); setCustomUrl(false); } else { set("url", generatedUrl); setCustomUrl(true); } }}>{customUrl ? "Use generated URL" : "Edit URL"}</button></div>
      </div>
      <div className="row center">
        <button type="button" className="btn" disabled={busy !== null} onClick={() => void testConnection()}>
          {busy === "test" && <span className="spinner" aria-hidden />}
          Test Connection
        </button>
      </div>
    </>
  );

  const options = (
    <>
      <Heading>Connection</Heading>
      <Check label="Read-only" checked={draft.read_only} onChange={(v) => set("read_only", v)} />
      <Choice
        label="Transaction control"
        value={draft.transaction}
        onChange={(v) => set("transaction", v)}
        options={[
          ["Auto", "auto"],
          ["Manual", "manual"],
        ]}
      />
      <Choice
        label="Isolation"
        value={draft.isolation}
        onChange={(v) => set("isolation", v)}
        options={[
          ["Database Default", "database-default"],
          ["Read Uncommitted", "read-uncommitted"],
          ["Read Committed", "read-committed"],
          ["Repeatable Read", "repeatable-read"],
          ["Serializable", "serializable"],
        ]}
      />
      <Choice
        label="Switch schema"
        value={draft.switch_schema}
        onChange={(v) => set("switch_schema", v)}
        options={[
          ["Automatic", "automatic"],
          ["Manual", "manual"],
          ["Disabled", "disabled"],
        ]}
      />
      <Choice
        label="Session control"
        value={draft.session_control}
        onChange={(v) => set("session_control", v)}
        options={[
          ["Single shared session", "single"],
          ["Dedicated sessions", "dedicated"],
          ["Two sessions: shared and introspection", "separate-introspection"],
        ]}
      />
      <Check
        label="Single database mode"
        checked={draft.single_database_mode}
        onChange={(v) => set("single_database_mode", v)}
      />
      {text("time_zone", "Time zone", "e.g. UTC")}
      {text("keep_alive_secs", "Run keep-alive query each (sec.)", "seconds")}
      {text("keep_alive_query", "Keep-alive query", "select 1")}
      {text("auto_disconnect_secs", "Auto-disconnect after (sec.)", "seconds")}
      {area("startup_script", "Startup script", "Statements run right after connecting", 3)}
      <Heading>Introspection</Heading>
      <Check label="Auto sync" checked={draft.auto_sync} onChange={(v) => set("auto_sync", v)} />
      <Choice
        label="Load sources for"
        value={draft.load_sources}
        onChange={(v) => set("load_sources", v)}
        options={[
          ["All schemas", "all-schemas"],
          ["All excl. system schemas", "all-excl-system"],
          ["None", "none"],
        ]}
      />
      <Check
        label="Warn when editing outdated DDL"
        checked={draft.warn_outdated}
        onChange={(v) => set("warn_outdated", v)}
      />
      <Choice
        label="Introspection level"
        value={draft.level}
        onChange={(v) => set("level", v)}
        options={[
          ["Auto select", "auto"],
          ["Level 1: Names only", "names"],
          ["Level 2: Skip sources", "skip-sources"],
          ["Level 3: Everything", "everything"],
        ]}
      />
      {text("external_data_file", "Virtual objects and attributes", "path to a metadata file")}
      <Note>Applied today: read-only, startup script. The other options are saved for upcoming engine support.</Note>
    </>
  );

  const sshSection = (
    <>
      <Check label="Use SSH tunnel" checked={draft.use_ssh} onChange={(value) => setDraft((before) => before ? { ...before, use_ssh: value, ssh_new: before.ssh_new || !sshNames.length, ssh_existing: before.ssh_existing || sshNames[0] || "" } : before)} />
      {draft.use_ssh && (
        <>
          <div className="field">
            <label className="field-label" htmlFor="saved-ssh-config">SSH configuration</label>
            <div className="row">
              <SelectField id="saved-ssh-config" aria-label="SSH configuration" className="tw:flex-1 tw:min-w-0" value={draft.ssh_new ? "" : draft.ssh_existing} disabled={busy !== null || !sshNames.length} onValueChange={ssh_existing => setDraft({ ...draft, ssh_new: false, ssh_existing })} placeholder={draft.ssh_new ? "New SSH configuration" : "Select a saved configuration"} options={sshNames.map(name => ({ value: name, label: name }))} />
              <button type="button" className="btn" disabled={busy !== null} aria-pressed={draft.ssh_new} onClick={() => set("ssh_new", true)}>New…</button>
            </div>
          </div>
          {text("ssh_local_port", "Forced local port (optional)", "a free port")}
          {draft.ssh_new && (
            <>
              {text("ssh_new_name", "Configuration name", "e.g. bastion")}
              <div className="row">
                <div className="grow">{text("ssh_host", "SSH host", "ssh.example.com")}</div>
                <div className="fixed narrow">{text("ssh_port", "Port", "22")}</div>
              </div>
              {text("ssh_user", "User")}
              <Choice
                label="Authentication"
                value={draft.ssh_auth}
                onChange={(v) => set("ssh_auth", v)}
                options={[
                  ["Password", "password"],
                  ["Key pair", "key-pair"],
                  ["SSH agent", "agent"],
                ]}
              />
              {draft.ssh_auth === "password" &&
                passwordField("ssh_password", "SSH password")}
              {draft.ssh_auth === "key-pair" && (
                <>
                  {text("ssh_key_file", "Private key file", "~/.ssh/id_ed25519")}
                  {passwordField("ssh_passphrase", "Key passphrase")}
                </>
              )}
              {draft.ssh_auth === "agent" && (
                <Note>Uses the keys of the running SSH agent (SSH_AUTH_SOCK).</Note>
              )}
              <Choice
                label="Unknown host key"
                value={draft.ssh_host_key}
                onChange={(v) => set("ssh_host_key", v)}
                options={[
                  ["Refuse (strict)", "strict"],
                  ["Trust and remember", "accept-new"],
                ]}
              />
              {text("ssh_known_hosts_file", "Known hosts file", "~/.ssh/known_hosts")}
              <div className="row"><button type="button" className="btn" disabled={busy !== null} onClick={() => void saveSshConfig()}>{busy === "ssh" ? "Saving SSH configuration…" : "Save SSH configuration"}</button></div>
              <Note>Saved in ssh.toml for reuse across data sources. Passwords and key passphrases are stored in Keychain.</Note>
            </>
          )}
          <Note>
            Applied today: the tunnel itself (password, key pair, SSH agent), strict host keys, forced local
            port, one tunnel per host in a cluster. A changed host key is always refused. Through a tunnel,
            TLS “Auto” does not add TLS: the traffic is already inside SSH up to the SSH host.
          </Note>
        </>
      )}
    </>
  );

  const sshSsl = (
    <>
      <Heading>SSL</Heading>
      <Choice
        label="Mode"
        value={draft.ssl_mode}
        onChange={(v) => set("ssl_mode", v)}
        options={[
          ["Auto", "auto"],
          ["Disable", "disable"],
          ["Require", "require"],
          ["Verify CA", "verify-ca"],
          ["Full Verification", "verify-full"],
        ]}
      />
      {text("ca_file", "CA file", "path to a CA certificate (PEM)")}
      {text("cert_file", "Client certificate file", "path to the client certificate (PEM)")}
      {text("key_file", "Client key file", "path to the client key (PEM)")}
      {passwordField("key_password", "Client key password")}
      <Note>
        Applied today: Auto, Disable and Full Verification on PostgreSQL. Require and Verify CA are saved but
        refused at connect time; certificate files are saved, not yet used.
      </Note>
      <Heading>SSH tunnel</Heading>
      {sshSection}
    </>
  );

  const schemas = (
    <>
      {text("schema_pattern", "Schema pattern", "e.g. @:* or *:public|db1:s1,s2")}
      <Note>
        Syntax: db1:sc1,sc2|db2:sc3. Names: @ default database/schema, * every one, {"{regexp}"} names
        matching. Examples: *:* everything, @:@ default schema, *:dbo|@:@|db1:s1,s2. Applied after
        introspection.
      </Note>
      {text("object_filter", "Object filter", "e.g. table:-tmp_.*||view:v_.*")}
      <Note>
        A pattern, or type:[-]pattern separated by ||, a minus excluding. Example:
        table:-OLD_.*||routine:DO.*. Types here: table, view, materialized view, sequence, routine. Applied
        after introspection.
      </Note>
    </>
  );

  const advanced = (
    <>
      {area("properties", "Driver properties", "One name=value per line", 8)}
      <Note>
        One name=value per line; blank lines and lines starting with # are ignored. Saved with the data
        source; not yet passed to the driver.
      </Note>
    </>
  );

  const bodies: Record<Tab, React.ReactNode> = {
    general,
    options,
    "ssh-ssl": sshSsl,
    schemas,
    advanced,
  };

  return (
    <dialog
      ref={dialog}
      className="dialog"
      aria-labelledby="source-dialog-title"
      onClose={onClose}
      onClick={(e) => e.target === dialog.current && dialog.current?.close()}
    >
      <form onSubmit={(e) => { if (mode === "drivers") { e.preventDefault(); void saveDriver(); } else { void submit(e); } }} className="dialog-form">
        <header className="dialog-head">
          <h2 id="source-dialog-title">{sourceName ? "Connection properties" : "Data Sources and Drivers"}</h2>
          <div className="dialog-modes" aria-label="Configuration section"><button type="button" className={mode === "sources" ? "on" : ""} onClick={() => setMode("sources")}>Data Sources</button><button type="button" className={mode === "drivers" ? "on" : ""} onClick={() => { setMode("drivers"); if (!editedDriver && current) editDriver(current); }}>Drivers</button></div>
          {mode === "sources" && <div className="tabs" role="tablist">
            {TABS.map(([label, id]) => (
              <button
                type="button"
                role="tab"
                key={id}
                aria-selected={tab === id}
                className={`tab${tab === id ? " on" : ""}`}
                onClick={() => setTab(id)}
              >
                {label}
              </button>
            ))}
          </div>}
        </header>
        <div className="source-dialog-content">
          <aside className="database-sidebar" aria-label="Database types">
            <div className="database-search"><input className="input" type="search" aria-label="Search databases" placeholder="Search databases" value={search} onChange={(e) => setSearch(e.target.value)} /></div>
            <ul className="database-catalog">{shown.map((d) => <li key={d.id}><button type="button" className={`driver${(mode === "sources" ? d.id === draft.driver : d.id === editedDriver?.id) ? " active" : ""}`} aria-pressed={mode === "sources" ? d.id === draft.driver : d.id === editedDriver?.id} onClick={() => mode === "sources" ? void chooseDriver(d.id) : editDriver(d)}><svg className="database-icon" viewBox="0 0 18 18" fill="none" aria-hidden="true"><ellipse cx="9" cy="4" rx="6" ry="2.5"/><path d="M3 4v9c0 1.4 2.7 2.5 6 2.5s6-1.1 6-2.5V4M3 8.5C3 9.9 5.7 11 9 11s6-1.1 6-2.5"/></svg><span>{d.name}</span>{d.custom && <span className="custom-label tw:text-meta tw:text-muted-foreground tw:ml-auto">custom</span>}</button></li>)}{!shown.length && <li className="driver-none">No databases match “{search}”.</li>}</ul>
            <div className="database-sidebar-foot"><button type="button" className="btn" onClick={newDriver}>Add Custom Driver…</button><span>{drivers.length} drivers</span></div>
          </aside>
          <div className="dialog-body" role={mode === "sources" ? "tabpanel" : undefined}>
            {mode === "sources" ? bodies[tab] : editedDriver ? <DriverEditor driver={editedDriver} settings={settings} existing={drivers.find((d) => d.id === editedDriver.id)} busy={driverBusy} onChange={setEditedDriver} onSave={() => void saveDriver()} onRemove={() => void removeDriver()} onUpload={(files) => void uploadDriverJars(files)} /> : <Note>Select a driver to configure, or add a custom driver.</Note>}
          </div>
        </div>
        {status.kind === "error" && <ErrorNotice error={status.text} title="Data source operation failed" context={draft.name || current?.name} />}
        <footer className="dialog-foot">
          <span className={`message ${status.kind}`} role="status">
            {status.kind === "error" ? "See error details above" : status.text}
          </span>
          <button type="button" className="btn" onClick={() => dialog.current?.close()}>
            Cancel
          </button>
          <button type={mode === "sources" ? "submit" : "button"} className="btn primary" disabled={busy !== null || driverBusy} onClick={mode === "drivers" ? () => setMode("sources") : undefined}>
            {busy === "save" && <span className="spinner" aria-hidden />}
            {mode === "drivers" ? "Done" : sourceName ? "Save changes" : "OK"}
          </button>
        </footer>
      </form>
    </dialog>
  );
}
