/** Preserve driver diagnostics and JavaScript causes across the command boundary. */
export function errorText(error: unknown): string {
  const seen = new Set<unknown>();
  const describe = (value: unknown): string => {
    if (value === null || value === undefined) return "No error details were provided.";
    if (typeof value === "string") return value.trim() || "No error details were provided.";
    if (typeof value !== "object") return String(value);
    if (seen.has(value)) return "[Circular error reference]";
    seen.add(value);
    if (value instanceof Error) {
      return value.message + (value.cause ? `\nCaused by: ${describe(value.cause)}` : "");
    }
    return Object.entries(value).map(([key, detail]) => `${key}: ${describe(detail)}`).join("\n") || "No error details were provided.";
  };
  return describe(error);
}

export function errorGuidance(message: string): string {
  if (/authentication failed|password authentication|access denied for user|invalid password|login failed/i.test(message)) return "Check the username, password, and authentication method in the data source settings, then test the connection again.";
  if (/permission denied|not authorized|insufficient privilege|access denied/i.test(message)) return "Check that your database user has permission for this operation. Ask a database administrator for the required access.";
  if (/connection refused|econnrefused/i.test(message)) return "Check the host and port, confirm the database server is running, and verify any SSH tunnel or VPN before reconnecting.";
  if (/timed? ?out|timeout|etimedout/i.test(message)) return "Check network access, VPN, firewall, and SSH tunnel settings. If the server is reachable, check its load and timeout settings before trying again.";
  if (/resolve|unknown host|enotfound|name or service not known/i.test(message)) return "Check the host name and DNS settings. Connect to the required network or VPN, then reconnect.";
  if (/certificate|ssl|tls/i.test(message)) return "Check the SSL mode, certificate trust, and certificate expiry in the data source settings.";
  if (/syntax error|parse error|unexpected token/i.test(message)) return "Review the SQL near the reported token or position. Check spelling, quotes, and syntax for this database dialect, then run it again.";
  if (/no database selected/i.test(message)) return "Choose a schema in the console toolbar, then run the query again.";
  if (/does not exist|no such table|unknown column|unknown database|invalid object name/i.test(message)) return "Check the object name and selected schema. Refresh the database catalog to confirm that the table or column exists.";
  if (/unique constraint|duplicate key|foreign key|not.null constraint|violates .*constraint/i.test(message)) return "Review the constraint named in the database message and correct the conflicting or missing values before running the statement again.";
  return "Review the reported details below and correct the SQL or data source settings involved before trying the operation again.";
}
