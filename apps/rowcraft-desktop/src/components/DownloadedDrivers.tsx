import { useEffect, useRef, useState } from "react";
import { api, errorText, type DownloadInventory, type DownloadedDriver } from "../api";
import { DriverDownload } from "./DriverDownload";
import { Icon } from "./Icon";
import { Field, Note } from "./form";
import { SelectField } from "./SelectField";

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function DownloadedDrivers({ onBusyChange }: { onBusyChange(busy: boolean): void }) {
  const [inventory, setInventory] = useState<DownloadInventory | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [selected, setSelected] = useState("");
  const mounted = useRef(false);
  const request = useRef(0);
  const mutating = useRef(false);

  async function refresh() {
    const token = ++request.current;
    setLoading(true); setError("");
    try {
      const next = await api.downloadedDrivers();
      if (mounted.current && token === request.current) setInventory(next);
    } catch (error) { if (mounted.current && token === request.current) setError(errorText(error)); }
    finally { if (mounted.current && token === request.current) setLoading(false); }
  }
  useEffect(() => {
    mounted.current = true;
    void refresh();
    return () => { mounted.current = false; request.current++; };
  }, []);

  function changeBusy(value: boolean) {
    mutating.current = value;
    if (mounted.current) {
      if (value) { request.current++; setLoading(false); }
      setBusy(value); onBusyChange(value);
    }
  }
  async function remove(pkg: DownloadedDriver) {
    if (mutating.current) return;
    changeBusy(true); setError(""); setMessage("");
    try {
      await api.removeDownloadedDriver(pkg.id);
      if (!mounted.current) return;
      setInventory(before => before ? { ...before, packages: before.packages.filter(item => item.id !== pkg.id) } : before);
      setConfirm(null); setExpanded(null);
      // Remount the downloader if it was displaying the removed package's installed state.
      if (selected === pkg.id) setSelected("");
      setMessage(`${pkg.name} removed. Connection settings are kept.`);
    } catch (error) { if (mounted.current) setError(errorText(error)); }
    finally { changeBusy(false); }
  }
  const packages = inventory?.packages ?? [];
  const available = inventory?.available.filter(driver => !packages.some(pkg => pkg.id === driver.id)) ?? [];
  return <section className="downloaded-drivers" aria-labelledby="downloaded-drivers-heading">
    <div className="tw:flex tw:flex-wrap tw:items-start tw:justify-between tw:gap-[12px]">
      <div><h3 id="downloaded-drivers-heading">Database drivers</h3><Note>PostgreSQL, MySQL, SQLite and other native connectors come with Rowcraft. Downloaded JDBC drivers are stored separately on this device.</Note></div>
      <button type="button" className="btn" disabled={busy || loading} onClick={() => void refresh()}><Icon name="refresh" />Refresh</button>
    </div>
    {error && <p role="alert" className="tw:m-0 tw:text-destructive tw:break-words">{error}</p>}
    {loading && <p role="status" className="note">Checking downloaded drivers…</p>}
    {inventory && <>
      <details className="native-drivers">
        <summary>Built-in connectors · {inventory.native.length} included with app</summary>
        <Note>PostgreSQL and other native connectors are compiled into Rowcraft. They work without downloading JDBC files and cannot be removed separately.</Note>
        <ul className="native-driver-list">{inventory.native.map(driver => <li key={driver.id}><span>{driver.name}</span><span className="note">Built in</span></li>)}</ul>
      </details>
      <h3>Downloaded JDBC drivers</h3>
      <div className="downloaded-drivers-summary"><span>{packages.length} {packages.length === 1 ? "driver" : "drivers"}</span><span>{formatSize(packages.reduce((sum, pkg) => sum + pkg.size, 0))} stored</span></div>
      <div className="note tw:break-all"><span>Driver storage: </span>{inventory.directory}</div>
      {!packages.length && <div className="downloaded-drivers-empty"><strong>No downloaded drivers</strong><Note>Choose a JDBC driver below to download it. Drivers downloaded during connection setup also appear here.</Note></div>}
      {packages.map(pkg => <article key={pkg.id} className="downloaded-driver" aria-label={pkg.name}>
        <div className="downloaded-driver-row">
          <button type="button" className="downloaded-driver-toggle" aria-expanded={expanded === pkg.id} aria-controls={`downloaded-${pkg.id}`} disabled={busy} onClick={() => { setExpanded(expanded === pkg.id ? null : pkg.id); setConfirm(null); }}>
            <Icon name={expanded === pkg.id ? "chevronDown" : "chevronRight"} />
            <span className="tw:min-w-0"><strong>{pkg.name}</strong><span className="note tw:block">v{pkg.version} · {pkg.files.length} {pkg.files.length === 1 ? "file" : "files"} · {formatSize(pkg.size)}</span></span>
          </button>
          <span className="note">{pkg.verified ? "Installed" : pkg.version !== pkg.available_version ? "Update available" : "Needs repair"}</span>
          <button type="button" className="btn" disabled={busy} aria-label={`Remove ${pkg.name}`} onClick={() => setConfirm(pkg.id)}>Remove</button>
        </div>
        {confirm === pkg.id && <div className="downloaded-driver-confirm" role="group" aria-label={`Confirm removal of ${pkg.name}`}>
          <Note>Remove {pkg.name} and its downloaded files? Download it again before reconnecting to databases that use it.</Note>
          <div className="row tw:flex-wrap"><button type="button" className="btn" disabled={busy} onClick={() => void remove(pkg)}>Remove driver</button><button type="button" className="btn" disabled={busy} onClick={() => setConfirm(null)}>Keep driver</button></div>
        </div>}
        {expanded === pkg.id && <div id={`downloaded-${pkg.id}`} className="downloaded-driver-details">
          <p className="note tw:m-0 tw:break-all">{pkg.directory}</p>
          <ul className="downloaded-driver-files">{pkg.files.map(file => <li key={file.path}><span title={file.path}>{file.name}</span><span className="note">{formatSize(file.size)}</span></li>)}</ul>
          <DriverDownload id={pkg.id} onBusyChange={changeBusy} onInstalled={() => { setMessage(`${pkg.name} downloaded.`); void refresh(); }} />
        </div>}
      </article>)}
      {!!available.length && <div className="downloaded-driver-install">
        <Field label="Download a driver"><SelectField aria-label="Download a driver" value={selected} disabled={busy} onValueChange={setSelected} placeholder="Choose a database" options={available.map(driver => ({ value: driver.id, label: `${driver.name} · ${driver.version}` }))} /></Field>
        {selected && <DriverDownload key={selected} id={selected} onBusyChange={changeBusy} onInstalled={() => { setSelected(""); setMessage("Driver downloaded and ready for connections."); void refresh(); }} />}
      </div>}
      <Note>Manually uploaded JARs and custom driver configurations are managed in Data Sources and Drivers.</Note>
    </>}
    {message && <p className="note tw:m-0" role="status">{message}</p>}
  </section>;
}
