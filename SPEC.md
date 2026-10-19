# Rowcraft: Specification

Status: draft v0.2 · 2026-10-15

Decisions locked in (v0.2):
- **The database engine is Rust; the desktop uses Tauri 2 and a web frontend.** Native connectors need no external database client runtime; JDBC connectors use an independent JDK. The UI uses the platform webview; see §3.1.
- **Database coverage is maintained in the app-owned driver catalog** (§5).
- **A headless CLI ships from day one** and shares the same core as the GUI (§4.8).

---

## 1. Product architecture

### 1.1 Runtime

Rowcraft is a standalone database workbench with a Rust engine, a Tauri desktop
shell, a React frontend, and a shared headless CLI. The desktop uses the platform
webview. Native connectors execute directly in Rust. JDBC connectors run through
an isolated, independently installed Java runtime and upstream vendor packages.

### 1.2 Database coverage

The app-owned database and download catalog lives in `drivers/catalog.json`.
Dedicated dialects cover PostgreSQL, Redshift, Greenplum, CockroachDB, MySQL,
MariaDB, SQLite, SQL Server, Sybase ASE, Oracle, Db2, Derby, H2, HSQLDB, ClickHouse,
Vertica, Exasol, Snowflake, BigQuery, Hive, Spark, Cassandra, MongoDB, Redis,
Couchbase, and DynamoDB. Additional protocols and generic JDBC expand the catalog.
Catalog presence does not guarantee that every vendor feature has been tested.

### 1.3 Design principles

1. Dialects supply introspection, type mapping, SQL grammar, and connection behavior.
2. Completion, navigation, and the explorer share a cached metadata model.
3. Database work runs outside the UI thread with explicit session lifetimes.
4. Incremental refresh can use database change markers where available; it remains
   a planned capability rather than a claimed completed feature.
5. Driver packages come directly from upstream suppliers and live in app-owned storage.

---

## 2. Product goals

| Goal | Target |
|---|---|
| Binary size | < 60 MB with all drivers (each driver is a cargo feature, so slim builds are possible) |
| Cold start to usable window | < 300 ms |
| Idle RAM with one connection | < 120 MB |
| Grid scroll | 60 fps over 1M-row results (virtualized, paged fetch) |
| Platforms | macOS (arm64/x64), Linux (x64/arm64), Windows (x64) |

**Non-goals for v1:** PL/SQL debugger, ER diagrams, cloud explorers, AI assistant, VCS integration, data-flow inspections.

### 2.1 What the Rust engine requirement means here

- **Allowed:** any crate in the Rust dependency tree, including crates that compile vendored C via `cc` at build time and link it statically. Two are unavoidable: **SQLite** (`rusqlite` with the `bundled` feature) and **ring** or **aws-lc-rs** under rustls. The user never installs anything.
- **Native database engine:** Rust connectors avoid separately installed database client libraries. Optional JDBC support runs in an isolated Java subprocess; ODBC requires a separately configured bridge.
- **Generic JDBC:** upstream packages are installed into app-owned storage. Availability and connection support are recorded separately.
- **TLS:** `rustls` only. SQL Server before 2016 and some old Sybase/Oracle servers need TLS 1.0/1.1, which rustls refuses. They are explicitly unsupported over TLS, and the UI shows a clear error.
- **Kerberos/NTLM/GSSAPI** (SQL Server integrated auth, Hive/Spark SASL, Db2): `sspi` crate (Devolutions, pure Rust).
- **AWS/GCP auth** (Redshift IAM, DynamoDB, BigQuery): `aws-sdk-*` with rustls, and `gcp_auth`/`yup-oauth2`.

---

## 3. Architecture

```
┌──────────────── rowcraft-desktop (Tauri 2 + React) ───────────────┐   ┌──── rowcraft (CLI) ────┐
│ Explorer tree │ SQL editor (rope + tree-sitter) │ Result grid      │   │ clap, comfy-table │
└──────────────────────────────▲─────────────────────────────────────┘   └────────▲─────────┘
                               │ Tauri IPC → Rust API                           │
┌──────────────────────────────┴───────────── rowcraft-core (tokio) ─────────────────────┴────────┐
│ sessions, consoles, query jobs, cancellation, history, events                              │
│ rowcraft-sql      splitter, tree-sitter highlight, completion, formatter (per-dialect flavor)    │
│ rowcraft-model    catalog model · rowcraft-cache: SQLite metadata cache + incremental refresh          │
│ rowcraft-dialect  trait Dialect / Connection + 26 dialect crates                                  │
│ rowcraft-wire-*   protocol crates we write ourselves (DRDA, TDS 5.0, TNS/TTC, Vertica, Exasol…)  │
│ rowcraft-script   boa_engine (pure-Rust JS) for the Mongo shell console and user extractors      │
│ transport   rustls · russh (SSH tunnels) · sspi (Kerberos/NTLM)                            │
│ rowcraft-io       export/import · rowcraft-config: TOML data sources + OS keychain (keyring)           │
└────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 3.1 UI framework: Tauri 2 (supersedes the gpui spike)

The desktop is a Tauri 2 shell around React, TypeScript and Vite. CodeMirror supplies the SQL editor;
TanStack Virtual renders only visible result rows. CSS tokens follow the OS light/dark preference.
The database engine and drivers remain Rust, shared unchanged with the CLI.

- `apps/rowcraft-desktop/src/`: frontend components, typed IPC wrappers, theme and development-only mocks.
- `apps/rowcraft-desktop/src-tauri/`: native window, local-window capabilities, CSP and thin command bridge.
- `crates/rowcraft-ui-model/`: UI-independent form validation, configuration persistence, sessions and result DTOs.
- `pnpm tauri dev`: real app with Vite hot reload. `pnpm dev`: browser-only UI iteration with mock data.
- `pnpm tauri build`: build frontend assets, embed them, compile Rust and bundle the desktop application.

Database operations run asynchronously on the Tauri tokio runtime; passwords are resolved in Rust.
Results are capped at 1,000 rows. Large integers travel as strings to avoid JavaScript precision loss;
NULL and binary placeholders remain distinguishable from text. The production webview has no remote
content, filesystem or shell plugin access. Node and pnpm are build-time tools only.

The previous gpui benchmark is not evidence about this implementation. Startup, memory and real-result
scrolling must be remeasured against the targets in §2. The migration prioritizes easier UI development;
it does not claim a proven runtime speedup over GPU-native gpui.

### 3.2 Crate layout (Cargo workspace)

```
crates/
  rowcraft-core            # sessions, jobs, events; no UI deps
  rowcraft-model           # Catalog, Schema, Table, Column, Index, FK, View, Routine, Sequence, Trigger …
  rowcraft-cache           # SQLite-backed metadata cache, per data source
  rowcraft-dialect         # Dialect + Connection traits, shared introspection helpers, Value/LogicalType
  rowcraft-sql             # splitter, completion engine, formatter; wraps sqlparser-rs + tree-sitter
  rowcraft-grid            # result paging, value formatting, edit buffer → DML
  rowcraft-io              # extractors/importers
  rowcraft-script          # boa_engine host (Mongo shell, custom extractors)
  rowcraft-ssh             # tunnels (russh)
  rowcraft-config          # data sources, settings, keychain
  rowcraft-ui-model        # UI-agnostic view models (explorer tree, tabs, grid state)
  dialects/
    pg, redshift, greenplum, cockroach      # share rowcraft-dialect-pgbase
    mysql, mariadb                          # share rowcraft-dialect-mysqlbase
    sqlite, mssql, sybase, oracle, db2, derby, h2, hsqldb,
    clickhouse, vertica, exasol, snowflake, bigquery, hive, spark,
    cassandra, mongo, redis, couchbase, dynamo
  wire/
    rowcraft-wire-drda       # Db2 + Derby (DRDA, Open Group spec)
    rowcraft-wire-tds5       # Sybase ASE (TDS 5.0)
    rowcraft-wire-tns        # Oracle Net / TTC thin protocol
    rowcraft-wire-vertica    # Vertica frontend/backend protocol (pg-v3 derived)
    rowcraft-wire-exasol     # Exasol WebSocket JSON API
    rowcraft-wire-hs2        # HiveServer2 Thrift (Hive + Spark Thrift Server)
    rowcraft-wire-hsqldb     # HSQLDB HSQL/HTTP protocol
    rowcraft-wire-h2         # H2 native TCP (until then: H2's PG server mode via pg driver)
apps/
  rowcraft-desktop         # Tauri app + web frontend
  rowcraft-cli             # `rowcraft` binary
```

### 3.3 Key interfaces

```rust
#[async_trait]
pub trait Dialect: Send + Sync {
    fn id(&self) -> DialectId;
    fn family(&self) -> DialectFamily;              // Sql, Document (Mongo/Couchbase), KeyValue (Redis/Dynamo), WideColumn (Cassandra)
    fn url_templates(&self) -> &[UrlTemplate];
    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>>;
    async fn introspect(&self, conn: &mut dyn Connection, scope: IntrospectScope,
                        since: Option<ChangeMarker>) -> Result<CatalogDelta>;
    fn map_type(&self, native: &NativeType) -> LogicalType;
    fn ddl(&self) -> Option<&dyn DdlGenerator>;     // CREATE/ALTER/DROP for the table editor
    fn explain(&self, sql: &str) -> Option<String>;
    fn sql_flavor(&self) -> SqlFlavor;              // quoting, keywords, splitter rules, parser dialect
    fn capabilities(&self) -> Capabilities;         // transactions, cancel, editable results, EXPLAIN …
}

#[async_trait]
pub trait Connection: Send {
    async fn execute(&mut self, stmt: &str, params: &[Value]) -> Result<ExecResult>;
    async fn query_stream(&mut self, stmt: &str, fetch_size: usize) -> Result<RowStream>;
    fn cancel_handle(&self) -> Box<dyn CancelHandle>; // out-of-band: pg cancel key, TDS attention, Oracle break …
    async fn begin(&mut self) -> Result<()>;
    async fn commit(&mut self) -> Result<()>;
    async fn rollback(&mut self) -> Result<()>;
}
```

Results stream as column batches (`Vec<ColumnChunk>`) to keep memory flat. The grid fetches pages of about 500 rows on demand, and the default auto-limit is 500 rows.

---

## 4. Feature spec

### 4.1 Data sources
- Create, edit and test connections with host/port/db/user/password or a URL. SSL modes, SSH tunnel (key, agent or password), read-only flag, auto-commit vs manual transaction, startup script, color/tag.
- Cloud auth where relevant: AWS IAM (Redshift, DynamoDB), GCP service account/ADC (BigQuery), Snowflake key-pair JWT and OAuth.
- Secrets go in the OS keychain (`keyring` crate). Config lives in `~/.config/rowcraft/datasources.toml`, and a per-project `.rowcraft/datasources.toml` can be committed without secrets.
- Import external connection definitions through explicit migration tools.

### 4.2 Explorer
- Tree: data source → database → schema → tables/views/matviews/routines/sequences → columns/indexes/keys/triggers. Document stores show collections/buckets, Redis shows keyspaces/key patterns.
- Lazy load from the cache. "Refresh" runs incremental introspection where the database exposes change markers (for pg, `xmin`/`pg_class`; elsewhere, last-DDL timestamps), and a per-schema re-scan otherwise.
- Schema filter (pattern list) per data source, because introspecting everything is the main slow path on big servers.
- Context actions: Open data, Copy name/DDL, Generate SELECT/INSERT/UPDATE, Drop (with confirmation), Truncate.

### 4.3 Query console and editor
- One console per data source plus scratch `.sql` files. Per-console session, schema switcher and transaction indicator.
- Console language per family: SQL (all SQL databases), CQL (Cassandra), PartiQL (DynamoDB), N1QL/SQL++ (Couchbase), **mongosh-style JS (MongoDB, run on `boa_engine`)**, Redis command console.
- Run statement at caret (dialect-aware splitter: `$$` bodies, `DELIMITER`, `GO`, Oracle `/`, Db2 `@`), run selection, run script. Cancel with Esc or the stop button.
- Parameters: `:name`, `?`, `${var}` prompt dialog.
- Highlighting with tree-sitter-sql plus per-dialect keyword tables. Parsing with `sqlparser-rs`, which covers pg, mysql, sqlite, mssql, snowflake, bigquery, clickhouse, hive, redshift and ansi. **We contribute or add flavors for Oracle, Db2, Vertica, Exasol and Sybase.**
- Completion: keywords, schema objects from the cache, columns resolved through FROM/JOIN aliases and CTEs, JOIN ON suggestions from foreign keys. Target: < 30 ms p95.
- Formatter: configurable keyword case and indent.
- Query history (SQLite, FTS5), snippets.
- Safety: warn on UPDATE/DELETE without WHERE, and on DDL against data sources marked production.

### 4.4 Result grid
- Virtualized grid with column resize/reorder/hide, sorting (client-side on the fetched page, or re-query with ORDER BY), and a filter row (WHERE clause injection).
- Multiple result tabs per run, pinning, and transposed (record) view. Document results show as a tree or JSON view.
- Cell viewers: text, JSON (pretty), binary (hex/image), long-text panel.
- Editing: in-place edit, add row, delete row. Changes are buffered → DML preview → submit in a transaction. Requires a PK/unique key, otherwise read-only.
- Selection aggregates in the status bar (COUNT, SUM, AVG, MIN, MAX, MEDIAN).
- Copy as TSV/CSV/Markdown/SQL INSERT/JSON.

### 4.5 Export and import
- Extractors: CSV, TSV, JSON, XML, Markdown, HTML, SQL INSERT (single and multi-row), XLSX (`rust_xlsxwriter`). Export streams a full query to file, not just the fetched page.
- User-defined extractors in JS on `boa_engine`.
- Import CSV/JSON/XLSX (`calamine`) into a table: column mapping, type inference preview, batch insert or fast path (`COPY`, `LOAD DATA`, TDS bulk load).
- **Dump/restore is native, with no shelling out to pg_dump.** It uses our own logical dump (DDL from the model + streamed INSERT/COPY). It is not byte-compatible with `pg_dump` custom format.

### 4.6 Table editor and schema compare (v1.1)
- Modify table dialog: columns, PK, indexes, FKs → `DdlGenerator` → preview SQL → apply.
- Compare two schemas from cached models → migration script.

### 4.7 EXPLAIN (v1.1)
- Per-dialect EXPLAIN in JSON/tree form, rendered as a tree with cost/rows bars, highlighting the most expensive node.

### 4.8 CLI (`rowcraft`), from day one
The same `rowcraft-core`, with no GUI dependencies. It reads the same `datasources.toml` and keychain.

```
rowcraft ds list | add | edit | rm | test <name>
rowcraft query  -d <ds> "SELECT …"   [--format table|csv|tsv|json|ndjson|md|sql] [--limit N] [-o file]
rowcraft run    -d <ds> script.sql   [--continue-on-error] [--tx]
rowcraft export -d <ds> --table s.t | --query "…"  --format csv -o out.csv
rowcraft import -d <ds> --table s.t data.csv [--create] [--mapping map.toml]
rowcraft introspect -d <ds> [--schema s] [--refresh]          # warms the cache, prints the tree
rowcraft ddl    -d <ds> s.t                                   # generated DDL
rowcraft dump   -d <ds> [--schema s] -o dump.sql              # native logical dump
rowcraft diff   -d <ds-a> -d <ds-b> [--schema s]              # v1.1
rowcraft shell  -d <ds>                                       # REPL with completion (reedline)
```

- Exit codes: 0 ok, 1 SQL error, 2 connection/auth error, 3 usage.
- Machine output (`json`/`ndjson`) is stable and versioned, so `rowcraft` can be scripted in CI.
- `rowcraft shell` uses `reedline` and the same `rowcraft-sql` completion engine as the GUI.

---

## 5. Database support: dedicated dialects and upstream JDBC packages

| Tier | Database | Rust implementation | Effort |
|---|---|---|---|
| **A: crate exists** | PostgreSQL | `tokio-postgres` (+ `postgres-native` types) | S |
| | Redshift, Greenplum, CockroachDB | pg wire via `rowcraft-dialect-pgbase` + dialect introspection | S each |
| | MySQL, MariaDB | `mysql_async` | S |
| | SQLite | `rusqlite` (bundled) | S |
| | SQL Server | `tiberius` (rustls) + `sspi` for integrated auth | M |
| | ClickHouse | `clickhouse` (HTTP, RowBinary) or our own native TCP later | S |
| | Cassandra | `scylla` (CQL v4/v5) | M |
| | MongoDB | `mongodb` + `boa_engine` shell shim | M |
| | Redis | `redis` (RESP2/3, cluster) | S |
| | DynamoDB | `aws-sdk-dynamodb` (PartiQL `ExecuteStatement`) | S |
| | H2 | via H2's PostgreSQL server mode (`-pg`) on the pg driver; native TCP later | S / M |
| **B: thin HTTP/RPC client** | Snowflake | REST query API + Arrow result chunks (`arrow-ipc`), JWT key-pair auth | M |
| | BigQuery | Jobs REST API + Storage Read API (gRPC via `tonic`) | M |
| | Couchbase | Query service HTTP (`:8093`) + management REST for introspection | M |
| | Exasol | WebSocket JSON API (`tokio-tungstenite`), documented and stable | M |
| | Hive, Spark | HiveServer2 Thrift (binary + HTTP transport, SASL PLAIN/Kerberos), shared `rowcraft-wire-hs2` | M |
| **C: protocol from scratch** | Vertica | Frontend/backend protocol, pg-v3 derived; fork of our pg wire handling | M |
| | Sybase ASE | TDS 5.0 (public FreeTDS/SAP protocol docs) | L |
| | Db2 (LUW, z/OS, i) | DRDA (Open Group DDM/DRDA/FD:OCA specs) | XL |
| | Derby | DRDA as well, reuses `rowcraft-wire-drda` | S (after Db2) |
| | Oracle | Oracle Net (TNS) + TTC "thin" protocol, including O5LOGON/verifier auth, native network encryption | XL |
| | HSQLDB | HSQL/HTTP protocol | M |

Effort: S ≈ 1–2 wk, M ≈ 3–5 wk, L ≈ 2 mo, XL ≈ 3–4 mo for one engineer, including introspection and tests.

**Clean-room rule for Tier C:** implement from published specs and observed wire behavior (pcap against a test server). Do not port code from GPL implementations. Permissively licensed open implementations (e.g. Oracle's own thin drivers) may be read as references subject to their license, and that decision is recorded per crate.

---

## 6. Persistence layout

```
~/.config/rowcraft/
  settings.toml            # UI, keymap, formatter, editor prefs
  datasources.toml         # global data sources (no secrets)
~/.local/share/rowcraft/     (platform-appropriate via `directories`)
  cache/<ds-id>.sqlite     # introspected catalog + change markers
  history.sqlite           # query history, FTS5
  consoles/<ds-id>/*.sql   # console contents (autosaved)
  extractors/*.js          # user extractors
```

---

## 7. Milestones

| # | Scope | Exit criteria |
|---|---|---|
| **M0** (2 wk) | Workspace, `Dialect`/`Connection` traits, pg driver, **`rowcraft query` / `rowcraft ds` working**, gpui spike (editor + 1M-row table) | CLI queries pg in CI on 3 OSes; spike meets §3.1 bar |
| **M1** (4 wk) | GUI shell: explorer from cache, console run/cancel, read-only grid with paging. Dialects: pg, mysql, mariadb, sqlite. CLI: `run`, `export`, `introspect`, `shell` | Browse and query a 5k-table pg DB, explorer opens in < 1 s from cache |
| **M2** (4 wk) | Completion, formatter, history, SSH tunnels, keychain, grid editing → DML, export/import | Completion p95 < 30 ms on 5k tables; 1M-row CSV export in constant memory |
| **M3** (6 wk) | Rest of Tier A: Redshift, Greenplum, CockroachDB, SQL Server, ClickHouse, Cassandra, MongoDB (+JS shell), Redis, DynamoDB, H2 (pg mode) | Dialect test matrix green against containers |
| **M4** (8 wk) | Tier B: Snowflake, BigQuery, Couchbase, Exasol, Hive, Spark. Table editor, EXPLAIN, schema compare, `rowcraft dump`/`rowcraft diff` | Same, plus cloud integration tests (sandbox accounts) |
| **M5** (parallel track, ~6–9 mo) | Tier C: Vertica → Sybase ASE → Db2 + Derby → Oracle → HSQLDB | Each ships independently behind a cargo feature as it reaches the test bar |
| **M6** | Packaging (dmg/msi/AppImage/Homebrew/cargo-dist), auto-update, size/start/RAM targets | §2 targets met |

Tiers A and B (21 of 26 databases) land in about 6 months for one engineer. Tier C is the long pole. It runs as a parallel track, and Oracle and Db2 are each a project in their own right.

---

## 8. Testing
- Dialect integration tests against Docker containers (`testcontainers`) for every database with an image: pg 13–17, MySQL 8/9, MariaDB 11, MSSQL 2022, ClickHouse, Cassandra, MongoDB, Redis, Couchbase, Exasol, Vertica CE, Db2 Community, Oracle Free, Sybase ASE dev, Hive, Spark, Derby, H2, HSQLDB. DynamoDB uses DynamoDB Local.
- Snowflake and BigQuery: nightly tests against sandbox accounts, plus recorded-response tests for CI.
- Wire crates: pcap-replay tests (recorded server responses), plus fuzzing of message decoders (`cargo fuzz`).
- Golden tests for the statement splitter and formatter, plus a completion corpus (SQL with a caret marker → expected items).
- CLI: snapshot tests of `--format json` output (`insta`).
- Benchmarks (criterion): introspecting a synthetic 10k-table schema, completion latency, grid page decode.

---

## 9. Open questions
1. Is statically linked vendored C (SQLite, ring/aws-lc) acceptable under "pure Rust"? (Assumed **yes** in §2.1. The alternative is `limbo`/Turso for SQLite, which is not yet production-grade.)
2. Tier C priority order: the default is Vertica → Sybase → Db2/Derby → Oracle → HSQLDB. Move Oracle first if it matters most to you.
3. Licensing of the clone itself (MIT/Apache-2.0 dual is the Rust default).
