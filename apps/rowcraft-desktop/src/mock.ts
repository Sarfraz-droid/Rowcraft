// A stand-in backend for `pnpm dev` in a plain browser, so the UI can be designed and iterated on
// without building Rust or opening a database. Never bundled into the app: main.tsx imports this
// only in dev mode and only when Tauri's IPC is missing.

import { mockIPC } from "@tauri-apps/api/mocks";

import type { AddedSource, Bootstrap, Catalog, Cell, Draft, DriverInfo, FormDefaults, QueryOutput, SourceSummary, CustomDriverConfig } from "./api";

import catalog from "./driver-catalog.json";
import downloadCatalog from "../../../drivers/catalog.json";

// Snapshot of rowcraft-drivers' built-in catalog, for the explicitly labeled browser demo only.
let DRIVERS: DriverInfo[] = catalog;
const customDrivers: CustomDriverConfig[] = [];
const sshNames = ["bastion"];
const DIALECTS = [...new Set(catalog.map((d) => d.dialect))].sort().map((id) => ({ id, name: id === "jdbc" ? "JDBC" : id }));

const sources: SourceSummary[] = [
  { name: "local pg", dialect: "pg", host: "localhost", ssh_tunnel: null, color: null },
  { name: "analytics (prod)", dialect: "pg", host: "db.example.com", ssh_tunnel: "bastion", color: "#d1242f" },
];

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function defaultDraft(): Draft {
  return {
    name: "", comment: "", color: "", connection_type: "", driver: "postgresql", dialect: "pg",
    host: "localhost", port: "", database: "", user: "", password_env: "", password: "", url: "",
    auth: "user-password", read_only: false, transaction: "auto", isolation: "database-default",
    switch_schema: "automatic", session_control: "single", single_database_mode: false,
    time_zone: "", keep_alive_secs: "", keep_alive_query: "", auto_disconnect_secs: "",
    startup_script: "", auto_sync: true, load_sources: "all-excl-system", warn_outdated: true,
    level: "auto", external_data_file: "", ssl_mode: "auto", ca_file: "", cert_file: "",
    key_file: "", key_password_env: "", key_password: "", schema_pattern: "", object_filter: "", properties: "",
    use_ssh: false, ssh_existing: "", ssh_new: false, ssh_local_port: "", ssh_new_name: "",
    ssh_host: "", ssh_port: "", ssh_user: "", ssh_auth: "password", ssh_key_file: "",
    ssh_password_env: "", ssh_password: "", ssh_passphrase_env: "", ssh_passphrase: "", ssh_host_key: "strict", ssh_known_hosts_file: "",
  };
}

function sampleResult(statement: string): QueryOutput {
  const rows: Cell[][] = Array.from({ length: 1000 }, (_, i) => [
    i + 1,
    `customer-${i + 1}@example.com`,
    i % 7 === 0 ? null : Math.round(i * 13.37 * 100) / 100,
    i % 2 === 0,
    i % 11 === 0 ? { bytes: 16 } : i % 5 === 0 ? "9223372036854775807" : "first line\nsecond line",
  ]);
  return {
    columns: ["id", "email", "total", "active", "notes"],
    rows,
    summary: `1000 rows (first 1000 only) in 12 ms  [mock: ${statement.slice(0, 30)}]`,
  };
}

const CATALOG: Catalog = {
  database: "app",
  schemas: [
    {
      name: "public",
      tables: [
        {
          name: "customers",
          kind: "Table",
          columns: [
            { name: "id", native_type: "integer", nullable: false },
            { name: "email", native_type: "text", nullable: false },
            { name: "total", native_type: "numeric(10,2)", nullable: true },
          ],
        },
        {
          name: "active_customers",
          kind: "View",
          columns: [{ name: "id", native_type: "integer", nullable: false }],
        },
      ],
    },
  ],
};

export function installMock(): void {
  mockIPC(async (cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    switch (cmd) {
      case "ai_settings": return { base_url: "http://127.0.0.1:20128/v1", model: "", fallback_models: [], has_api_key: false };
      case "ai_router_status": return { installed: false, running: false, managed: false, version: "0.5.104", pid: null, base_url: null };
      case "save_ai_settings":
      case "ai_models":
      case "ai_complete":
      case "ai_read_schema":
      case "ai_readonly_query":
      case "start_ai_router":
      case "stop_ai_router":
      case "open_ai_dashboard": throw new Error("AI provider configuration is available in the desktop app. The browser preview does not run local services.");
      case "import_sqlite":
        throw "SQLite file uploads require the desktop app. Launch Rowcraft to import and query your database.";
      case "bootstrap":
        return { sources: [...sources], startup_error: "Browser demo — database connections require the desktop app." } satisfies Bootstrap;
      case "drivers":
        return DRIVERS;
      case "driver_package": {
        const driver = downloadCatalog.drivers.find(driver => driver.id === args.id);
        if (!driver) throw "Unknown built-in driver.";
        return { distribution: driver.distribution, version: "version" in driver ? driver.version : null,
          source: "source" in driver ? driver.source : null, installed: driver.distribution === "native", files: [] };
      }
      case "download_driver":
      case "open_driver_vendor":
        throw "Driver downloads require the desktop app. Open Rowcraft to install this driver.";
      case "downloaded_drivers":
        return { directory: "Available in the desktop app", packages: [], native: downloadCatalog.drivers
          .filter(driver => driver.distribution === "native").map(driver => ({ id: driver.id, name: driver.name, version: null })), available: downloadCatalog.drivers
          .filter(driver => driver.distribution === "maven")
          .map(driver => ({ id: driver.id, name: driver.name, version: "version" in driver ? driver.version : null })) };
      case "remove_downloaded_driver":
        throw "Driver management requires the desktop app.";
      case "driver_settings":
        return { custom: [...customDrivers], dialects: DIALECTS };
      case "save_driver": {
        const driver = args.driver as CustomDriverConfig;
        if (!driver.name.trim()) throw "Driver name is required.";
        if (!/^[a-z0-9_-]+$/.test(driver.id)) throw "Use lowercase letters, numbers, underscores or hyphens for the driver ID.";
        if (DRIVERS.some((d) => d.id === driver.id && !d.custom)) throw "This ID belongs to a built-in driver. Choose a unique ID.";
        if (driver.port !== null && (!Number.isInteger(driver.port) || driver.port < 1 || driver.port > 65535)) throw "Port must be between 1 and 65535.";
        if (driver.dialect === "jdbc" && (!driver.jdbc_url_template?.startsWith("jdbc:") || !driver.jdbc_jars.some((p) => p.trim()))) throw "JDBC requires a URL template starting with jdbc: and at least one JAR file.";
        const i = customDrivers.findIndex((d) => d.id === driver.id);
        if (i < 0) customDrivers.push(driver); else customDrivers[i] = driver;
        DRIVERS = [...DRIVERS.filter((d) => d.id !== driver.id), { id: driver.id, name: driver.name, dialect: driver.dialect, port: driver.port, file: driver.target === "file", custom: true, caveat: null }].sort((a, b) => a.name.localeCompare(b.name));
        return DRIVERS;
      }
      case "remove_driver": {
        const i = customDrivers.findIndex((d) => d.id === args.id);
        if (i < 0) throw "Custom driver not found.";
        customDrivers.splice(i, 1);
        DRIVERS = DRIVERS.filter((d) => d.id !== args.id);
        return DRIVERS;
      }
      case "form_defaults":
        return { draft: defaultDraft(), ssh_names: [...sshNames] } satisfies FormDefaults;
      case "edit_source": {
        const source = sources.find(item => item.name === args.name);
        if (!source) throw "Data source not found.";
        return { draft: { ...defaultDraft(), name: source.name, dialect: source.dialect, host: source.host ?? "", color: source.color ?? "", use_ssh: !!source.ssh_tunnel, ssh_existing: source.ssh_tunnel ?? "" }, ssh_names: [...sshNames], has_saved_password: false, has_saved_key_password: false };
      }
      case "update_source": {
        const draft = args.draft as Draft;
        const index = sources.findIndex(item => item.name === args.originalName);
        if (index < 0) throw "Data source not found.";
        const name = draft.name.trim();
        if (!name) throw "Name is required.";
        if (sources.some((item, position) => position !== index && item.name === name)) throw "A data source with this name already exists.";
        sources[index] = { name, dialect: draft.dialect, host: draft.host || null, ssh_tunnel: draft.use_ssh ? draft.ssh_existing : null, color: draft.color || null };
        return { name, sources: [...sources] } satisfies AddedSource;
      }
      case "select_driver": {
        const draft = args.draft as Draft;
        const driver = DRIVERS.find((d) => d.id === args.id);
        if (!driver) return draft;
        return { ...draft, driver: driver.id, dialect: driver.dialect ?? "jdbc", host: draft.host || (driver.file ? "" : "localhost") };
      }
      case "save_ssh_config": {
        const draft = args.draft as Draft;
        const name = draft.ssh_new_name.trim();
        if (!name || !draft.ssh_host.trim() || !draft.ssh_user.trim()) throw "SSH configuration name, host and user are required.";
        if (sshNames.includes(name)) throw "An SSH configuration with this name already exists.";
        throw "Saving SSH configurations requires the desktop app.";
      }
      case "add_source": {
        await wait(150);
        const draft = args.draft as Draft;
        const name = draft.name.trim();
        if (!name) throw "Name is required.";
        if (sources.some((s) => s.name === name)) throw `A data source named \`${name}\` already exists.`;
        sources.push({ name, dialect: draft.dialect, host: draft.host || null, ssh_tunnel: null, color: null });
        return { sources: [...sources], name } satisfies AddedSource;
      }
      case "test_connection":
      case "connect":
        throw "Browser demo cannot connect to databases. Launch the desktop app to test this connection.";
      case "remove_source": {
        const i = sources.findIndex((s) => s.name === args.name);
        if (i < 0) throw `No data source named \`${String(args.name)}\`.`;
        sources.splice(i, 1);
        return [...sources];
      }
      case "introspect":
        await wait(300);
        return CATALOG;
      case "preview_statement":
        return `select * from "${String(args.schema)}"."${String(args.table)}" limit 1000`;
      case "run_query": {
        await wait(250);
        const statement = String(args.statement);
        if (/error/i.test(statement)) throw `query failed: syntax error near "error" in ${statement}`;
        return sampleResult(statement);
      }
      case "save_export":
        return `~/Downloads/${String(args.fileName)}`;
      case "reveal_file":
        return null;
      default:
        throw `unknown command ${cmd}`;
    }
  });
}
