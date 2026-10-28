/** curl commands for the local HTTP API started by `rowcraft serve` (docs/api.md). */

export const DEFAULT_API_PORT = 7878;
export const API_PORT_KEY = "rowcraft.apiPort";

export type ApiSnippet = { id: string; label: string; command: string };

/** Quotes `text` as one POSIX shell word. */
export function shellQuote(text: string): string {
  return `'${text.replace(/'/g, `'\\''`)}'`;
}

/** Path segment for a data source name, matching the server's percent-decoding. */
export function sourcePath(source: string): string {
  return `/datasources/${encodeURIComponent(source)}`;
}

export function apiSnippets({ source, sql, port = DEFAULT_API_PORT }: { source: string; sql: string; port?: number }): ApiSnippet[] {
  const base = `http://127.0.0.1:${port}`;
  const path = `${base}${sourcePath(source)}`;
  const statement = sql.trim() || "SELECT 1";
  return [
    { id: "serve", label: "Start the API server", command: port === DEFAULT_API_PORT ? "rowcraft serve" : `rowcraft serve --listen 127.0.0.1:${port}` },
    { id: "query", label: "Run SQL (JSON result)", command: `curl -s -X POST ${shellQuote(`${path}/query`)} \\\n  -H 'Content-Type: application/json' \\\n  -d ${shellQuote(JSON.stringify({ sql: statement, max_rows: 100 }))}` },
    { id: "csv", label: "Run SQL (CSV result)", command: `curl -s -X POST ${shellQuote(`${path}/query?format=csv`)} \\\n  --data-binary ${shellQuote(statement)}` },
    { id: "catalog", label: "Read the catalog", command: `curl -s ${shellQuote(`${path}/catalog`)}` },
    { id: "test", label: "Test the connection", command: `curl -s -X POST ${shellQuote(`${path}/test`)}` },
    { id: "list", label: "List every data source", command: `curl -s ${base}/datasources` },
  ];
}
