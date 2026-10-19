import { useState } from "react";
import { errorGuidance } from "../errors";
import { Icon } from "./Icon";

interface Props {
  error: string;
  title?: string;
  context?: string;
  onRetry?: () => void;
  busy?: boolean;
}

export function ErrorNotice(props: Props) {
  // A new failure resets disclosure and clipboard feedback, including in retained tabs.
  return <ErrorContent key={props.error} {...props} />;
}

function ErrorContent({ error, title = "Operation failed", context, onRetry, busy }: Props) {
  const [copyStatus, setCopyStatus] = useState("");
  const [expanded, setExpanded] = useState(false);
  const summary = error.split(/\r?\n/).find(line => line.trim()) ?? "No error details were provided.";
  const guidance = errorGuidance(error);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText([title, context, error].filter(Boolean).join("\n\n"));
      setCopyStatus("Copied");
    } catch {
      setCopyStatus("Could not copy. Select and copy the details below.");
    }
  };
  return <section className="error-notice" aria-label={title}>
    <div className="error-notice-heading" role="alert"><Icon name="close" size={14} /><strong>{title}</strong>{context && <span className="error-notice-context">{context}</span>}</div>
    <p className="error-notice-cause" title={error}>{summary}</p>
    <p className="error-notice-guidance">{guidance}</p>
    <details className="error-notice-details" open={expanded}>
      <summary onClick={event => { event.preventDefault(); setExpanded(value => !value); }} title="Show the complete error reported by the driver, including any causes, codes, and positions">Show details</summary>
      {expanded && <><pre tabIndex={0} aria-label="Technical error details">{error}</pre>
      <button type="button" className="btn ghost" title="Copy the complete error and its context to the clipboard" onClick={() => void copy()}><Icon name="copy" size={12} />Copy error</button>
      <span className="error-copy-status" role="status">{copyStatus}</span></>}
    </details>
    {onRetry && <button type="button" className="btn ghost" onClick={onRetry} disabled={busy} title="Retry this operation after correcting the reported problem"><Icon name="refresh" size={12} />Try again</button>}
  </section>;
}
