import type { AiSettings, AiSettingsView, AiModel, AiMessage, AiReply, RouterStatus } from "./ai";
import { errorText } from "./errors";
export { errorText } from "./errors";
// Typed wrappers over the Tauri commands in src-tauri/src/lib.rs. The shapes mirror the serde
// types of the `rowcraft-ui-model` crate (enums are kebab-case there, field names snake_case).

import { Channel, invoke } from "@tauri-apps/api/core";

export interface DriverPackage {
  distribution: "native" | "maven" | "vendor";
  version: string | null;
  source: string | null;
  installed: boolean;
  files: string[];
}

export interface DownloadedDriver {
  id: string;
  name: string;
  version: string;
  available_version: string | null;
  verified: boolean;
  directory: string;
  files: { name: string; path: string; size: number }[];
  size: number;
}

export interface DownloadInventory {
  directory: string;
  packages: DownloadedDriver[];
  available: { id: string; name: string; version: string | null }[];
  native: { id: string; name: string; version: string | null }[];
}

export type AuthType = "user-password" | "no-auth" | "windows-credentials";
export type TransactionMode = "auto" | "manual";
export type Isolation =
  | "database-default"
  | "read-uncommitted"
  | "read-committed"
  | "repeatable-read"
  | "serializable";
export type SwitchSchema = "automatic" | "manual" | "disabled";
export type SessionControl = "single" | "dedicated" | "separate-introspection";
export type LoadSources = "all-schemas" | "all-excl-system" | "none";
export type IntrospectionLevel = "auto" | "names" | "skip-sources" | "everything";
export type SslMode = "auto" | "disable" | "require" | "verify-ca" | "verify-full";
export type SshAuth = "password" | "key-pair" | "agent";
export type HostKeyPolicy = "strict" | "accept-new";

/** The "Add data source" form, as typed. Free text stays text; the Rust side validates it. */
export interface Draft {
  // General
  name: string;
  comment: string;
  color: string;
  connection_type: string;
  driver: string;
  dialect: string;
  host: string;
  port: string;
  database: string;
  user: string;
  /** Name of the environment variable holding the password, never the password itself. */
  password_env: string;
  /** Entered directly; stored by the backend in the OS credential store. */
  password: string;
  clear_password?: boolean;
  url: string;
  auth: AuthType;
  // Options: connection
  read_only: boolean;
  transaction: TransactionMode;
  isolation: Isolation;
  switch_schema: SwitchSchema;
  session_control: SessionControl;
  single_database_mode: boolean;
  time_zone: string;
  keep_alive_secs: string;
  keep_alive_query: string;
  auto_disconnect_secs: string;
  startup_script: string;
  // Options: introspection
  auto_sync: boolean;
  load_sources: LoadSources;
  warn_outdated: boolean;
  level: IntrospectionLevel;
  external_data_file: string;
  // SSL
  ssl_mode: SslMode;
  ca_file: string;
  cert_file: string;
  key_file: string;
  key_password_env: string;
  key_password: string;
  clear_key_password?: boolean;
  // Schemas
  schema_pattern: string;
  object_filter: string;
  // Advanced: one `name=value` per line
  properties: string;
  // SSH tunnel
  use_ssh: boolean;
  ssh_existing: string;
  ssh_new: boolean;
  ssh_local_port: string;
  ssh_new_name: string;
  ssh_host: string;
  ssh_port: string;
  ssh_user: string;
  ssh_auth: SshAuth;
  ssh_key_file: string;
  ssh_password_env: string;
  ssh_password: string;
  ssh_passphrase_env: string;
  ssh_passphrase: string;
  ssh_host_key: HostKeyPolicy;
  ssh_known_hosts_file: string;
}

export interface SourceSummary {
  name: string;
  dialect: string;
  host: string | null;
  ssh_tunnel: string | null;
  color: string | null;
}

export interface SourceProperties {
  name: string; dialect: string; driver: string | null; host: string | null;
  port: number | null; database: string | null; user: string | null;
  connection_string: string; connection_type: string | null; comment: string | null;
  read_only: boolean; ssl_mode: SslMode; auth: AuthType; ssh_tunnel: string | null;
  schema_pattern: string | null; object_filter: string | null;
}

export interface Bootstrap {
  sources: SourceSummary[];
  startup_error: string | null;
}

export interface FormDefaults {
  draft: Draft;
  ssh_names: string[];
}

export interface SourceEditDefaults extends FormDefaults {
  has_saved_password: boolean;
  has_saved_key_password: boolean;
}

export interface AddedSource {
  sources: SourceSummary[];
  name: string;
}

export interface ImportedSqlite {
  path: string;
}

async function importSqlite(file: File): Promise<ImportedSqlite> {
  if (file.size > 256 * 1024 * 1024) {
    throw new Error("This database exceeds the 256 MB upload limit. Enter its local file path instead.");
  }
  try {
    return await invoke<ImportedSqlite>("import_sqlite", await file.arrayBuffer());
  } catch (e) {
    throw new Error(errorText(e));
  }
}

export interface CustomDriverConfig {
  id: string;
  name: string;
  dialect: string;
  port: number | null;
  target: "network" | "file";
  jdbc_class: string | null;
  jdbc_jars: string[];
  jdbc_url_template: string | null;
}

export interface DriverSettings {
  custom: CustomDriverConfig[];
  dialects: { id: string; name: string }[];
}

export interface DriverInfo {
  jdbc_url_template?: string | null;
  dialect: string | null;
  custom: boolean;
  id: string;
  name: string;
  port: number | null;
  /** A file database: the Database field holds a path, there is no host or port. */
  file: boolean;
  /** Why the driver cannot be used yet, if it cannot. */
  caveat: string | null;
}

/**
 * One cell: `null` is SQL NULL, a string is text, `{ bytes }` is binary data. Integers beyond
 * 2^53 arrive as text so they are never rounded.
 */
export type Cell = null | boolean | number | string | { bytes: number };

export interface QueryOutput {
  columns: string[];
  rows: Cell[][];
  /** Status-bar text such as `3 rows in 12 ms`. */
  summary: string;
  row_limit?: number | null;
}

export type TableKind = "Table" | "PartitionedTable" | "View" | "MaterializedView" | "ForeignTable";

export interface ColumnInfo {
  name: string;
  native_type: string;
  nullable: boolean;
  default?: string | null;
  comment?: string | null;
}

export interface ConstraintInfo {
  name: string;
  kind: "PrimaryKey" | "Unique" | "ForeignKey" | "Check" | "Exclusion";
  columns: string[];
  definition: string;
  reference: { schema: string; table: string; columns: string[]; on_update?: string; on_delete?: string } | null;
}

export interface IndexInfo {
  name: string;
  unique: boolean;
  primary: boolean;
  columns: string[];
  method?: string | null;
}

export interface TableInfo {
  name: string;
  kind: TableKind;
  columns: ColumnInfo[];
  constraints?: ConstraintInfo[];
  indexes?: IndexInfo[];
  comment?: string | null;
}

export interface SchemaInfo {
  name: string;
  tables: TableInfo[];
}

/** The part of the Rust `Catalog` the explorer shows. */
export interface Catalog {
  database: string | null;
  default_schema?: string | null;
  schemas: SchemaInfo[];
}

/** Commands reject with complete diagnostics for the error panels. */
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    throw new Error(errorText(e));
  }
}


export const api = {
  sourceProperties: (name: string) => call<SourceProperties>("source_properties", { name }),
  editSource: (name: string) => call<SourceEditDefaults>("edit_source", { name }),
  updateSource: (originalName: string, draft: Draft) => call<AddedSource>("update_source", { originalName, draft }),
  aiSettings: () => call<AiSettingsView>("ai_settings"),
  saveAiSettings: (settings: AiSettings, apiKey: string | null, clearApiKey: boolean) => call<AiSettingsView>("save_ai_settings", { settings, apiKey, clearApiKey }),
  aiModels: (settings: AiSettings, apiKey: string | null) => call<AiModel[]>("ai_models", { settings, apiKey }),
  aiComplete: (messages: AiMessage[], toolsEnabled = false, model?: string) => call<AiReply>("ai_complete", { messages, toolsEnabled, model }),
  aiReadSchema: (source: string, schema?: string) => call<Catalog>("ai_read_schema", { source, schema }),
  aiReadonlyQuery: (source: string, statement: string, schema?: string) => call<QueryOutput>("ai_readonly_query", { source, statement, schema }),
  aiRouterStatus: (baseUrl?: string) => call<RouterStatus>("ai_router_status", { baseUrl }),
  startAiRouter: (baseUrl: string) => call<RouterStatus>("start_ai_router", { baseUrl }),
  stopAiRouter: (baseUrl: string) => call<RouterStatus>("stop_ai_router", { baseUrl }),
  openAiDashboard: (settings: AiSettings) => call<void>("open_ai_dashboard", { settings }),
  saveExport: (fileName: string, contents: string) => call<string>("save_export", { fileName, contents }),
  revealFile: (path: string) => call<void>("reveal_file", { path }),
  importSqlite,
  importJdbcJar: async (file: File): Promise<string> => {
    if (!/\.jar$/i.test(file.name)) throw new Error("Choose a JDBC driver JAR file (.jar).");
    if (file.size > 256 * 1024 * 1024) throw new Error("This JAR exceeds the 256 MB upload limit. Enter its local file path instead.");
    try {
      return await invoke<string>("import_jdbc_jar", await file.arrayBuffer());
    } catch (e) {
      throw new Error(errorText(e));
    }
  },
  bootstrap: () => call<Bootstrap>("bootstrap"),
  drivers: () => call<DriverInfo[]>("drivers"),
  driverPackage: (id: string) => call<DriverPackage>("driver_package", { id }),
  downloadedDrivers: () => call<DownloadInventory>("downloaded_drivers"),
  removeDownloadedDriver: (id: string) => call<void>("remove_downloaded_driver", { id }),
  openDriverVendor: (id: string) => call<void>("open_driver_vendor", { id }),
  downloadDriver: (id: string, onProgress: (message: string) => void) => {
    const channel = new Channel<string>();
    channel.onmessage = onProgress;
    return call<DriverPackage>("download_driver", { id, onProgress: channel });
  },
  driverSettings: () => call<DriverSettings>("driver_settings"),
  saveDriver: (driver: CustomDriverConfig) => call<DriverInfo[]>("save_driver", { driver }),
  removeDriver: (id: string) => call<DriverInfo[]>("remove_driver", { id }),
  formDefaults: () => call<FormDefaults>("form_defaults"),
  selectDriver: (draft: Draft, id: string) => call<Draft>("select_driver", { draft, id }),
  addSource: (draft: Draft) => call<AddedSource>("add_source", { draft }),
  testConnection: (draft: Draft, originalName?: string) => call<null>("test_connection", { draft, originalName }),
  saveSshConfig: (draft: Draft) => call<string[]>("save_ssh_config", { draft }),
  connect: (name: string, reconnect?: boolean) => call<string>("connect", reconnect ? { name, reconnect: true } : { name }),
  removeSource: (name: string) => call<SourceSummary[]>("remove_source", { name }),
  introspect: () => call<Catalog>("introspect"),
  previewStatement: (schema: string, table: string) =>
    call<string>("preview_statement", { schema, table }),
  runQuery: (statement: string, schema?: string, showAll = false) => call<QueryOutput>("run_query", { statement, schema, showAll }),
};
