import { useEffect, useRef, useState } from "react";
import { api, errorText, type DriverPackage } from "../api";
import { Note } from "./form";

/** Shared by connection setup and built-in driver configuration. */
export function DriverDownload({ id, onInstalled, onBusyChange }: { id: string; onInstalled?(files: string[]): void; onBusyChange?(busy: boolean): void }) {
  const [state, setState] = useState<{ id: string; package?: DriverPackage; error?: string; busy?: boolean; progress?: string }>({ id });
  const activeId = useRef(id);
  const installing = useRef(false);
  useEffect(() => {
    activeId.current = id;
    let active = true;
    setState({ id });
    api.driverPackage(id).then(pkg => { if (active) setState({ id, package: pkg }); })
      .catch(error => { if (active) setState({ id, error: errorText(error) }); });
    return () => { active = false; activeId.current = ""; };
  }, [id]);
  const current = state.id === id ? state : { id };
  const pkg = current.package;
  const download = async () => {
    if (!pkg || installing.current) return;
    installing.current = true;
    onBusyChange?.(true);
    setState({ id, package: pkg, busy: true, progress: "Starting driver download…" });
    try {
      const installed = await api.downloadDriver(id, progress => {
        if (activeId.current === id) setState({ id, package: pkg, busy: true, progress });
      });
      if (activeId.current === id) {
        setState({ id, package: installed });
        onInstalled?.(installed.files);
      }
    } catch (error) {
      if (activeId.current === id) setState({ id, package: pkg, error: errorText(error) });
    } finally { installing.current = false; onBusyChange?.(false); }
  };
  if (!pkg && !current.error) return <Note>Checking driver files…</Note>;
  if (pkg?.distribution === "native") return null;
  return <div className="field" aria-label="Driver download" aria-busy={current.busy || undefined}>
    {pkg?.distribution === "maven" && <>
      <div className="row center tw:flex-wrap">
        <span className="note grow">{pkg.installed ? `Driver ${pkg.version} installed` : `JDBC driver ${pkg.version}`}</span>
        <button type="button" className="btn" disabled={current.busy} onClick={() => void download()}>
          {current.busy && <span className="spinner" aria-hidden="true" />}
          {current.busy ? "Downloading…" : pkg.installed ? "Download again" : "Download driver"}
        </button>
      </div>
      <Note>Downloads the driver and its dependencies from Maven Central. A standalone Java runtime is required to connect.</Note>
      {pkg.installed && <p className="note" role="status">Files are saved in Rowcraft’s storage and ready for connections.</p>}
    </>}
    {pkg?.distribution === "vendor" && <>
      <Note>This driver requires a vendor download. Upload the downloaded JARs in driver configuration.</Note>
      {pkg.source && <button type="button" className="btn" onClick={() => void api.openDriverVendor(id).catch(error => {
        if (activeId.current === id) setState({ id, package: pkg, error: errorText(error) });
      })}>Get driver from vendor</button>}
    </>}
    {current.busy && <p className="note tw:break-all" role="status">{current.progress}</p>}
    {current.error && <div role="alert"><p className="tw:text-destructive tw:break-words">{current.error}</p>{!pkg && <button type="button" className="btn" onClick={() => {
      setState({ id });
      void api.driverPackage(id).then(value => { if (activeId.current === id) setState({ id, package: value }); })
        .catch(error => { if (activeId.current === id) setState({ id, error: errorText(error) }); });
    }}>Retry driver check</button>}</div>}
  </div>;
}
