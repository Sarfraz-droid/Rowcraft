# Architecture

```
apps/rowcraft-desktop/src (React + CodeMirror + virtualized grid)
        │ Tauri IPC (typed command wrappers in src/api.ts)
apps/rowcraft-desktop/src-tauri (window + command bridge)
        │
        └─> rowcraft-ui-model (form validation, workbench service, result DTOs)
                 │
apps/rowcraft-cli ─────┴─> rowcraft-core ──> rowcraft-dialect (traits) <── dialects/* ──> wire/* (when no driver exists)
                         │
                         ├─> rowcraft-registry   (wires every dialect crate behind cargo features)
                         ├─> rowcraft-config     (data sources, settings)
                         ├─> rowcraft-model      (Catalog / Schema / Table / Column …)
                         ├─> rowcraft-cache      (SQLite metadata cache)
                         ├─> rowcraft-sql        (splitter, completion, formatter)
                         ├─> rowcraft-grid       (paging, value formatting, edit buffer → DML)
                         ├─> rowcraft-io         (export / import)
                         ├─> rowcraft-script     (embedded JS: Mongo shell, custom extractors)
                         └─> rowcraft-ssh        (tunnels)
```

The system webview renders the desktop frontend; database access stays in Rust. Only the main local
window receives Tauri capabilities. The frontend has no shell/filesystem plugin access, and its production
CSP limits network access to IPC. Secrets are resolved in Rust, not sent to the webview.

`pnpm dev` supports browser-only design with a development mock; `pnpm tauri dev` runs the real command
bridge. Form and result logic is tested without a webview in `rowcraft-ui-model`.

## Rules

1. `rowcraft-core` and below never depend on a UI crate or on a concrete dialect crate.
2. Only `rowcraft-registry` depends on dialect crates. Each is behind a cargo feature named after its id.
3. A dialect crate depends on `rowcraft-dialect` and `rowcraft-config`, plus a driver or a `wire/` crate.
4. `wire/*` crates know nothing about dialects: they speak a protocol and expose typed messages.
5. Dialect tiers (see SPEC.md §5): A = Rust crate exists, B = HTTP/RPC client, C = protocol from scratch.
