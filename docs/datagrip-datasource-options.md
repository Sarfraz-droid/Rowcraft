# DataGrip data source options: findings and our status

Source: DataGrip 2026.2.5 (build 262.10315.132), `/Applications/DataGrip.app`, plugin `DatabaseTools`.

## Method

- Tool: the REA 6.5.0 CLI (`rea inspect-artifact`, `rea extract-artifact`) run under Node 24.21, which
  REA needs (it refuses Node 20). No REA registration or setup was changed; the CLI only reads.
- REA listed the contents of each jar and extracted 104 jars into its own temporary directory. Only text
  resources were read: XML, `.properties`, JSON. Nothing from the bundle was executed.
- `grep -a` over jar central directories located the one jar that holds the label bundle
  (`plugins/DatabaseTools/lib/modules/intellij.database.jar`, `messages/DatabaseBundle.properties`, 3,413 keys).
- The REA MCP tools (`open_binary` then `inspect_artifact`) failed on `intellij.database.ssh.ui.jar` with
  "Root artifact changed during inventory", with and without `record-and-continue`, although the jar's
  SHA-256 was identical before and after and its modification time was old. The REA CLI inventoried the
  same jar without complaint, so the CLI results are what this document uses.
- Not done: decompiling classes. Anything that is only in code (default values, layout, field order, the
  SSH panel's own fields) is **unknown** here and is marked so below.

## Where the facts came from

| Fact | File inside the app | Evidence |
|---|---|---|
| Tab names, field labels, choices | `intellij.database.jar` : `messages/DatabaseBundle.properties` | keys `DataSourceConfigurable.tab.*`, `data.source.*`, `database.auth.*`, `database.driver.settings.*`, `transaction.mode.*`, `ssl.mode.*` |
| Schema pattern and object filter syntax | same bundle | `DataSourceSchemasPanel.schema.pattern.hint`, `DataSourceSchemasPanel.object.filter.hint` |
| 80 driver definitions: names, ports, URL templates, default properties, options | `databaseDrivers/*-drivers.xml` in each dialect jar (54 files) | `<driver id name dialect driver-class based-on>`, `<url-template>`, `<option>`, `<property>` |
| Introspection levels | same bundle | `database.introspection.level.*` |
| Incremental introspection design | `intellij.database.dialects.postgres.jar` : `introspector/PgIntroQueries.sql` | `RetrieveExistent*` and `age(xmin) <= #TXAGE` (see SPEC §1.3) |

## The dialog

DataGrip's data source dialog has five tabs, `DataSourceConfigurable.tab.*`: **General, Options, SSH/SSL,
Schemas, Advanced**. The driver has its own settings page (General, Options, Advanced).

Status: **Applied** = the engine acts on it. **Saved** = stored in `datasources.toml` and shown in the
form, but nothing uses it yet. **Refused** = saved, and connecting fails with a clear message.

### General

| DataGrip label | Our field | Status |
|---|---|---|
| Name | `name` | Applied (the key) |
| Comment | `comment` | Saved |
| Color | `color` (`#rrggbb`) | Saved |
| Connection type | `connection_type` (free text; DataGrip's value list is not in the text resources) | Saved |
| Driver | `driver` (catalog id), sets `dialect` | Applied |
| Host, Port | `host`, `port` | Applied |
| Authentication: User & Password / No auth / Windows credentials | `auth` | No auth: Applied (no password is sent). Windows credentials: Refused |
| User | `user` | Applied |
| Password | `password_env` (name of an environment variable) | Applied |
| Save: Never / Until restart / For session / Forever | none | Not built. Needs the OS keychain (SPEC M2) |
| Database | `database` | Applied |
| URL ("Overrides settings above") | `url` | Applied; a password in the URL is rejected |
| Test Connection | form button | Applied |

### Options

| DataGrip label | Our field | Status |
|---|---|---|
| Read-only | `read_only` | Applied on PostgreSQL |
| Transaction control: Auto / Manual | `options.transaction` | Saved |
| Isolation (Database Default … Serializable) | `options.isolation` | Saved |
| Switch schema: Automatic / Manual / Disabled | `options.switch_schema` | Saved |
| Session control: single shared / dedicated / two sessions | `options.session_control` | Saved |
| Single database mode | `options.single_database_mode` | Saved |
| Time zone | `options.time_zone` | Saved |
| Run keep-alive query each N sec. | `options.keep_alive_secs`, `keep_alive_query` | Saved |
| Auto-disconnect after N sec. | `options.auto_disconnect_secs` | Saved |
| Startup script | `options.startup_script` | Applied (runs once after connecting; a failure names the script) |
| Auto sync | `introspection.auto_sync` | Saved |
| Load sources for: All schemas / All excl. system schemas / None | `introspection.load_sources` | Saved. DataGrip's default is unknown; ours is "All excl. system schemas" |
| Warn when editing outdated DDL | `introspection.warn_outdated` | Saved |
| Introspection level: Auto / L1 names only / L2 skip sources / L3 everything | `introspection.level` | Saved |
| Virtual objects and attributes | `introspection.external_data_file` | Saved |

### SSH/SSL

| DataGrip label | Our field | Status |
|---|---|---|
| Use SSL, Mode (Require, Verify CA, Full Verification, …) | `ssl_mode`: auto, disable, require, verify-ca, verify-full | auto, disable, verify-full: Applied on PostgreSQL. require, verify-ca: Refused (rustls always verifies; we will not silently be stricter than asked) |
| CA file, Client certificate file, Client key file, Client key password | `ssl_files.*` | Saved (key password as an environment variable name) |
| Use SSH tunnel | `ssh_tunnel` (name of a shared configuration) | Applied (`dg-ssh`, pure Rust) |
| SSH configuration (a shared, reusable entry) | `ssh.toml`, managed by `dg ssh add/list/rm/test` and the form's "New…" | Applied. DataGrip's own field list is in the IDE platform, not DatabaseTools, so ours is the usual SSH client set: host, port, user, authentication (password, key pair, agent), key file, environment variable names for password and passphrase, host key policy, known-hosts file |
| Copy from… | none | Not built |
| Forced local port | `ssh_local_port` | Applied. An occupied port fails before any login; with several hosts it is refused, with DataGrip's own wording |

### Schemas

| DataGrip label | Our field | Status |
|---|---|---|
| Schema pattern | `schema_pattern` | Applied after introspection |
| Object filter | `object_filter` | Applied after introspection |

Schema pattern grammar, from DataGrip's own hint: entries separated by `|`, each `db:sc1,sc2`; a name is `@`
(default database/schema), `*` (all), `{regexp}` or a literal. Our rule for an entry without `:` (the hint
does not say) is "schemas in the default database". Object filter: a pattern, or `type:[-]pattern`
separated by `||`, a minus excluding; patterns match the whole name. The type names DataGrip accepts are
not in the text resources; we use `table`, `view`, `materialized view`, `sequence`, `routine`.

### Advanced

| DataGrip label | Our field | Status |
|---|---|---|
| VM options / environment / home path | none | Not applicable: there is no JVM |
| Driver properties | `properties` | Passed to JDBC drivers; reserved `jdbc.*` keys configure the bridge |

## Drivers

DataGrip defines 80 driver elements, including alternative JDBC implementations and version-specific
variants. Our catalog presents one entry per database family/connection target. Existing Rust connectors
remain native; remaining database types use a generic JDBC bridge, including the former dialect stubs.
JDBC needs Java 11+ and vendor JAR files. Each vendor still needs live validation against its server.

The dialog has a searchable database sidebar and a **Drivers** section for adding, editing and removing
custom native-compatible or JDBC drivers. JDBC configuration includes class, dependency JAR paths,
URL template, default port and file target. Built-in drivers are copied into custom entries for editing.
Driver definitions persist in `drivers.toml`; saved sources protect their driver's protocol/target from
changes and block its removal. The application uses its own UI implementation; exact DataGrip layout,
JDBC variant parity, automatic vendor downloads and OS keychain integration remain outside this change.

The bridge keeps one JDBC connection per session, carries credentials over stdin, supports result limits,
metadata introspection and connection options for read-only/transactions/isolation where supported by
the driver. It searches `<config-dir>/jdbc/<driver-id>/` and downloaded DataGrip JARs on macOS.

Some drivers need extra fields, visible in their URL templates: SQL Server instance name, Oracle SID or
service name (plus Thin/OCI variants), MongoDB `authSource`, `+srv` and read preference, Snowflake
authenticator. The catalog records which driver needs which (`Extra`); the form has no fields for them yet.

## Not covered yet

- Password storage modes and the keychain.
- SSH: what `dg-ssh` follows from the bundle (class names of `intellij.database.ssh.ui` and
  `intellij.database.ssh`, plus bundle keys): the tab picks a shared SSH configuration; a tunnel establisher
  asks for credentials again when authentication fails (up to 3 times), supports a forced local port (errors
  for "occupied" and "multiple tunnels"), and extracts every host:port so cluster databases get one tunnel per
  node. Still missing: an interactive prompt in the GUI for a rejected password (the engine already asks
  again through its `Credentials` hook; the GUI answers from environment variables only), reading
  `~/.ssh/config` (the bundle has a "Reading SSH Configuration" progress title, so DataGrip does), "Copy
  from…", certificate host keys, jump hosts, and the SSH configuration's own field list from the platform.
- The extra per-database fields above.
- Data source templates, "Data Source from URL / Path / Cloud", Dev Container environment, session templates.
- Editing or deleting an existing data source from the GUI.
- Enforcing the "Saved" options listed above.

## Licensing note

The JetBrains files were read to learn *what* the dialog offers. Their text is not copied: labels are
ours, the driver catalog is hand-written facts (names and ports), and the introspection SQL is written
against the public PostgreSQL catalogs.
