import { useEffect } from "react";
import { Icon } from "./Icon";

interface Props {
  path: string;
  onReveal: () => void;
  onClose: () => void;
}

/** Confirms a saved export and where it landed; dismisses itself after a few seconds. */
export function ExportToast({ path, onReveal, onClose }: Props) {
  useEffect(() => {
    const timer = setTimeout(onClose, 8000);
    return () => clearTimeout(timer);
  }, [path, onClose]);
  const name = path.split(/[\\/]/).pop() ?? path;
  return <div className="export-toast" role="status" aria-live="polite">
    <span className="export-toast-icon"><Icon name="check" size={14} /></span>
    <div className="export-toast-body">
      <strong>Saved {name}</strong>
      <span className="export-toast-path" title={path}>{path}</span>
    </div>
    <button type="button" className="btn ghost" onClick={onReveal}>Show in folder</button>
    <button type="button" className="icon-button" aria-label="Dismiss" onClick={onClose}><Icon name="close" size={12} /></button>
  </div>;
}
