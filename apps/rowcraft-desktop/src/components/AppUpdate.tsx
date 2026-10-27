import { useCallback, useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { isTauri } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { errorText } from "../api";
import { Icon } from "./Icon";

/** First automatic check after launch, then again while the app stays open. */
const FIRST_CHECK_MS = 5_000;
const RECHECK_MS = 6 * 60 * 60 * 1000;

type Phase =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; percent: number | null }
  | { kind: "installed" }
  | { kind: "failed"; error: string };

/**
 * Status-bar updater: checks the signed GitHub release feed (`plugins.updater` in tauri.conf.json),
 * downloads and installs a newer version when asked, then offers a restart into it.
 */
export function AppUpdate() {
  const [version, setVersion] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const phaseRef = useRef(phase);
  phaseRef.current = phase;

  /** Background checks stay quiet when offline; a manual check reports what happened. */
  const lookForUpdate = useCallback(async (manual: boolean) => {
    if (!["idle", "current", "failed"].includes(phaseRef.current.kind)) return;
    if (manual) setPhase({ kind: "checking" });
    try {
      const update = await check();
      if (update) setPhase({ kind: "available", update });
      else if (manual) {
        setPhase({ kind: "current" });
        setTimeout(() => setPhase(now => now.kind === "current" ? { kind: "idle" } : now), 3000);
      }
    } catch (error) {
      if (manual) setPhase({ kind: "failed", error: errorText(error) });
    }
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    void getVersion().then(setVersion, () => undefined);
    const first = setTimeout(() => void lookForUpdate(false), FIRST_CHECK_MS);
    const again = setInterval(() => void lookForUpdate(false), RECHECK_MS);
    return () => { clearTimeout(first); clearInterval(again); };
  }, [lookForUpdate]);

  const install = async (update: Update) => {
    let total = 0, received = 0;
    setPhase({ kind: "downloading", percent: null });
    try {
      await update.downloadAndInstall(event => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        else if (event.event === "Progress") {
          received += event.data.chunkLength;
          setPhase({ kind: "downloading", percent: total ? Math.min(100, Math.round(received / total * 100)) : null });
        }
      });
      setPhase({ kind: "installed" });
    } catch (error) {
      setPhase({ kind: "failed", error: errorText(error) });
    }
  };

  if (!version) return null;
  switch (phase.kind) {
    case "checking":
      return <span className="app-update" role="status"><Icon name="refresh" size={12} className="rotating" />Checking for updates…</span>;
    case "current":
      return <span className="app-update" role="status">Rowcraft {version} is up to date</span>;
    case "available":
      return <button type="button" className="app-update available" onClick={() => void install(phase.update)} title={[`Rowcraft ${phase.update.version} is available (you have ${version}).`, phase.update.body?.trim(), "Click to download and install it."].filter(Boolean).join("\n\n")}><Icon name="download" size={12} />Update to {phase.update.version}</button>;
    case "downloading":
      return <span className="app-update" role="status"><Icon name="download" size={12} />Downloading update{phase.percent === null ? "…" : ` ${phase.percent}%`}</span>;
    case "installed":
      return <button type="button" className="app-update available" onClick={() => void relaunch()} title="The update is installed. Restart Rowcraft to use it; unsaved work in open tabs is lost."><Icon name="refresh" size={12} />Restart to update</button>;
    case "failed":
      return <button type="button" className="app-update failed" onClick={() => void lookForUpdate(true)} title={`${phase.error}\n\nClick to check again.`}><Icon name="refresh" size={12} />Update failed · Retry</button>;
    default:
      return <button type="button" className="app-update" onClick={() => void lookForUpdate(true)} title="Check for updates">v{version}</button>;
  }
}
