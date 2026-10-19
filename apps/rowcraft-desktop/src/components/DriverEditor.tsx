import { useRef } from "react";
import { SelectField } from "./SelectField";
import type { CustomDriverConfig, DriverInfo, DriverSettings } from "../api";
import { Check, Field, Note, TextArea, TextField } from "./form";

const JDBC_PROTOCOLS = new Set(["jdbc", "oracle", "db2", "derby", "hsqldb", "sybase", "exasol", "hive", "spark", "bigquery"]);

export function DriverEditor({ driver, settings, existing, busy, onChange, onSave, onRemove, onUpload }: {
  driver: CustomDriverConfig;
  settings: DriverSettings;
  existing: DriverInfo | undefined;
  busy: boolean;
  onChange(driver: CustomDriverConfig): void;
  onSave(): void;
  onRemove(): void;
  onUpload(files: File[]): void;
}) {
  const jarInput = useRef<HTMLInputElement>(null);
  const set = <K extends keyof CustomDriverConfig>(key: K, value: CustomDriverConfig[K]) => onChange({ ...driver, [key]: value });
  const nativeDialects = settings.dialects.filter((d) => !JDBC_PROTOCOLS.has(d.id));
  return <>
    <div className="driver-detail-title"><h3>{existing?.custom ? "Custom driver" : "New driver"}</h3><span>Driver configuration</span></div>
    <TextField label="Name" value={driver.name} onChange={(v) => set("name", v)} placeholder="My database driver" />
    <Field label="ID"><input className="input" value={driver.id} readOnly={existing?.custom} onChange={(e) => set("id", e.target.value)} placeholder="my_database" spellCheck={false} /></Field>
    <Field label="Driver type"><SelectField aria-label="Driver type" value={driver.dialect === "jdbc" ? "jdbc" : "native"} onValueChange={value => set("dialect", value === "jdbc" ? "jdbc" : (nativeDialects[0]?.id ?? "pg"))} options={[{ value: "jdbc", label: "JDBC driver" }, { value: "native", label: "Native wire protocol" }]} /></Field>
    {driver.dialect === "jdbc" ? <>
      <TextField label="Class" value={driver.jdbc_class ?? ""} onChange={(v) => set("jdbc_class", v)} placeholder="com.example.jdbc.Driver" />
      <TextArea label="Driver files" value={driver.jdbc_jars.join("\n")} onChange={(v) => set("jdbc_jars", v.split("\n"))} placeholder="/path/to/database-driver.jar" rows={4} />
      <div className="row tw:flex-wrap">
        <button type="button" className="btn" disabled={busy} onClick={() => jarInput.current?.click()}>Upload JAR files…</button>
        <input ref={jarInput} type="file" accept=".jar" multiple hidden aria-label="Upload JDBC driver files" disabled={busy} onChange={(e) => {
          const files = Array.from(e.target.files ?? []);
          e.target.value = "";
          if (files.length) onUpload(files);
        }} />
      </div>
      <Note>Upload driver and dependency JARs (up to 256 MB each), or enter one absolute path per line. Uploaded files are copied to local app storage and loaded with your Java runtime.</Note>
      <TextField label="URL template" value={driver.jdbc_url_template ?? ""} onChange={(v) => set("jdbc_url_template", v)} placeholder="jdbc:database://{host}:{port}/{database}" />
      <Note>Template fields: {"{host}"}, {"{port}"}, {"{database}"}, {"{user}"}. Passwords come from the data source’s environment variable.</Note>
    </> : <Field label="Protocol"><SelectField aria-label="Protocol" value={driver.dialect} onValueChange={value => set("dialect", value)} options={nativeDialects.map(dialect => ({ value: dialect.id, label: dialect.name }))} /></Field>}
    <div className="row"><div className="grow"><Field label="Default port"><input className="input" type="number" min={1} max={65535} step={1} disabled={driver.target === "file"} value={driver.port ?? ""} onChange={(e) => set("port", e.target.value === "" ? null : Number(e.target.value))} placeholder="Optional" /></Field></div></div>
    <Check label="File-based database (no host or port)" checked={driver.target === "file"} onChange={(v) => onChange({ ...driver, target: v ? "file" : "network", port: v ? null : driver.port })} />
    <div className="row driver-actions"><button type="button" className="btn primary" disabled={busy} onClick={onSave}>Save Driver</button>{existing?.custom && <button type="button" className="btn" disabled={busy} onClick={onRemove}>Remove Driver</button>}</div>
  </>;
}
