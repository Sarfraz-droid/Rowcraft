# Architecture

```
apps/dg-desktop/src (React + CodeMirror + virtualized grid)
        │ Tauri IPC (typed command wrappers in src/api.ts)
apps/dg-desktop/src-tauri (window + command bridge)
        │
        └─> dg-ui-model (form validation, workbench service, result DTOs)
                 │
apps/dg-cli ─────┴─> dg-core ──> dg-dialect (traits) <── dialects/* ──> wire/* (when no driver exists)
                         │
                         ├─> dg-registry   (wires every dialect crate behind cargo features)
                         ├─> dg-config     (data sources, settings)
                         ├─> dg-model      (Catalog / Schema / Table / Column …)
                         ├─> dg-cache      (SQLite metadata cache)
                         ├─> dg-sql        (splitter, completion, formatter)
                         ├─> dg-grid       (paging, value formatting, edit buffer → DML)
                         ├─> dg-io         (export / import)
                         ├─> dg-script     (embedded JS: Mongo shell, custom extractors)
                         └─> dg-ssh        (tunnels)
```

The system webview renders the desktop frontend; database access stays in Rust. Only the main local
window receives Tauri capabilities. The frontend has no shell/filesystem plugin access, and its production
CSP limits network access to IPC. Secrets are resolved in Rust, not sent to the webview.

`pnpm dev` supports browser-only design with a development mock; `pnpm tauri dev` runs the real command
bridge. Form and result logic is tested without a webview in `dg-ui-model`.

## Rules

1. `dg-core` and below never depend on a UI crate or on a concrete dialect crate.
2. Only `dg-registry` depends on dialect crates. Each is behind a cargo feature named after its id.
3. A dialect crate depends on `dg-dialect` and `dg-config`, plus a driver or a `wire/` crate.
4. `wire/*` crates know nothing about dialects: they speak a protocol and expose typed messages.
5. Dialect tiers (see SPEC.md §5): A = Rust crate exists, B = HTTP/RPC client, C = protocol from scratch.
