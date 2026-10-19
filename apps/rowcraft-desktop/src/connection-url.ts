import type { Draft, DriverInfo } from "./api";

/** A password-free preview. Automatic connections continue to use the individual fields. */
export function connectionUrl(draft: Draft, driver?: DriverInfo): string {
  const port = draft.port.trim() || String(driver?.port ?? "");
  const database = draft.database.trim();
  const user = draft.user.trim();
  let host = draft.host.trim() || "localhost";
  if (host.includes(":") && !host.includes(",") && !host.startsWith("[")) host = `[${host}]`;
  if (["neo4j", "graphql"].includes(draft.dialect)) {
    const scheme = draft.ssl_mode === "disable" || (draft.ssl_mode === "auto" && ["localhost", "127.0.0.1", "[::1]"].includes(host)) ? "http" : "https";
    return `${scheme}://${host}${port ? `:${port}` : ""}${draft.dialect === "graphql" ? "/graphql" : `/db/${encodeURIComponent(database || "neo4j")}/query/v2`}`;
  }
  if (driver?.jdbc_url_template) {
    const values: Record<string, string> = { host, port, database, user };
    return driver.jdbc_url_template.replace(/\{(host|port|database|user)\}/g, (_, key: string) => values[key]!);
  }
  const scheme = ({ pg: "postgresql", mssql: "sqlserver", mongo: "mongodb" } as Record<string, string>)[draft.dialect] ?? draft.dialect;
  if (driver?.file) return `${scheme}://${database.startsWith("/") ? "" : "/"}${database.split("/").map(encodeURIComponent).join("/")}`;
  const auth = draft.auth === "user-password" && user ? `${encodeURIComponent(user)}@` : "";
  return `${scheme}://${auth}${host}${port ? `:${port}` : ""}/${encodeURIComponent(database)}`;
}

/** Best-effort form synchronization. Incomplete/opaque URLs stay under backend validation. */
export function parseConnectionUrl(value: string, existingProperties = ""): Partial<Draft> | null {
  try {
    const match = value.trim().match(/^(jdbc:)?([a-z][a-z0-9+.-]*):\/\/([^/?#]*)(\/[^?#]*)?(?:\?([^#]*))?$/i);
    if (!match) return null;
    const [, jdbc = "", scheme = "", authority = "", path = "", query = ""] = match;
    const patch: Partial<Draft> = {};
    const at = authority.lastIndexOf("@");
    let hosts = authority;
    let safeAuthority = authority;
    if (at >= 0) {
      hosts = authority.slice(at + 1);
      const info = authority.slice(0, at);
      const colon = info.indexOf(":");
      patch.user = decodeURIComponent(colon < 0 ? info : info.slice(0, colon));
      patch.auth = "user-password";
      if (colon >= 0) {
        patch.password = decodeURIComponent(info.slice(colon + 1));
        patch.password_env = "";
        patch.clear_password = false;
        safeAuthority = `${info.slice(0, colon)}@${hosts}`;
      }
    }
    const hostList = hosts.split(",");
    for (const host of hostList) {
      if (!host && scheme !== "sqlite" && scheme !== "file") return null;
      const parsed = host.match(/^(?:\[([^\]]+)\]|([^:\s]+))(?::(\d+))?$/);
      if (!parsed && host) return null;
      const port = parsed?.[3];
      if (port && (Number(port) < 1 || Number(port) > 65535)) return null;
      if (hostList.length === 1 && parsed) {
        patch.host = decodeURIComponent(parsed[1] ?? parsed[2] ?? "");
        if (port) patch.port = port;
      }
    }
    if (hostList.length > 1) patch.host = hosts;
    if (path.length > 1) patch.database = decodeURIComponent(scheme === "sqlite" || scheme === "file" ? path : path.slice(1));

    const properties = new Map<string, string>();
    for (const line of existingProperties.split("\n")) {
      const equal = line.indexOf("=");
      if (equal > 0) properties.set(line.slice(0, equal).trim(), line.slice(equal + 1));
    }
    const safeQuery: string[] = [];
    for (const pair of query.split("&").filter(Boolean)) {
      const equal = pair.indexOf("=");
      const key = decodeURIComponent(equal < 0 ? pair : pair.slice(0, equal));
      const parameter = decodeURIComponent(equal < 0 ? "" : pair.slice(equal + 1));
      if (/^(password|passwd|pwd)$/i.test(key)) {
        patch.password = parameter; patch.password_env = ""; patch.clear_password = false;
        continue;
      }
      safeQuery.push(pair);
      if (/[\r\n]/.test(key + parameter)) continue;
      properties.set(key, parameter);
      if (key.toLowerCase() === "user" && patch.user === undefined) { patch.user = parameter; patch.auth = "user-password"; }
      if (key === "sslmode" && ["disable", "require", "verify-ca", "verify-full"].includes(parameter)) patch.ssl_mode = parameter as Draft["ssl_mode"];
      if ((key === "tls" || key === "ssl") && /^(true|false)$/i.test(parameter)) patch.ssl_mode = parameter.toLowerCase() === "true" ? "verify-full" : "disable";
    }
    if (query) patch.properties = [...properties].map(([key, parameter]) => `${key}=${parameter}`).join("\n");
    patch.url = `${jdbc}${scheme}://${safeAuthority}${path}${safeQuery.length ? `?${safeQuery.join("&")}` : ""}`;
    return patch;
  } catch {
    // A partly typed percent escape must not erase the user's last valid field values.
    return null;
  }
}
