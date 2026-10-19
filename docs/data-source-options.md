# Rowcraft data source options

## The dialog

The data source dialog has five tabs: **General, Options, SSH/SSL,
Schemas, Advanced**. The driver has its own settings page (General, Options, Advanced).

Status: **Applied** = the engine acts on it. **Saved** = stored in `datasources.toml` and shown in the
form, but nothing uses it yet. **Refused** = saved, and connecting fails with a clear message.

### General

| Label | Our field | Status |
|---|---|---|
| Name | `name` | Applied (the key) |
| Comment | `comment` | Saved |
| Color | `color` (`#rrggbb`) | Saved |
| Connection type | `connection_type` (free text) | Saved |
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

| Label | Our field | Status |
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
| Load sources for: All schemas / All excl. system schemas / None | `introspection.load_sources` | Saved. Default: "All excl. system schemas" |
| Warn when editing outdated DDL | `introspection.warn_outdated` | Saved |
| Introspection level: Auto / L1 names only / L2 skip sources / L3 everything | `introspection.level` | Saved |
| Virtual objects and attributes | `introspection.external_data_file` | Saved |

### SSH/SSL

| Label | Our field | Status |
|---|---|---|
| Use SSL, Mode (Require, Verify CA, Full Verification, …) | `ssl_mode`: auto, disable, require, verify-ca, verify-full | auto, disable, verify-full: Applied on PostgreSQL. require, verify-ca: Refused (rustls always verifies; we will not silently be stricter than asked) |
| CA file, Client certificate file, Client key file, Client key password | `ssl_files.*` | Saved (key password as an environment variable name) |
| Use SSH tunnel | `ssh_tunnel` (name of a shared configuration) | Applied (`rowcraft-ssh`, pure Rust) |
| SSH configuration (a shared, reusable entry) | `ssh.toml`, managed by `rowcraft ssh add/list/rm/test` and the form's "New…" | Applied. Fields: host, port, user, authentication (password, key pair, agent), key file, environment variable names for password and passphrase, host key policy, known-hosts file |
| Copy from… | none | Not built |
| Forced local port | `ssh_local_port` | Applied. An occupied port fails before any login; with several hosts it is refused |

### Schemas

| Label | Our field | Status |
|---|---|---|
| Schema pattern | `schema_pattern` | Applied after introspection |
| Object filter | `object_filter` | Applied after introspection |

Schema patterns contain entries separated by `|`, each `db:sc1,sc2`. A name is `@`
(the default database/schema), `*` (all), `{regexp}`, or a literal. An entry without
`:` selects schemas in the default database. Object filters contain a pattern or
`type:[-]pattern`, separated by `||`; a minus excludes matching objects. Patterns
match the whole name. Supported types are `table`, `view`, `materialized view`,
`sequence`, and `routine`.

### Advanced

| Label | Our field | Status |
|---|---|---|
| VM options / environment / home path | none | Configure Java independently for JDBC connections |
| Driver properties | `properties` | Passed to JDBC drivers; reserved `jdbc.*` keys configure the bridge |

## Drivers

The catalog presents one entry per database family/connection target. Existing Rust
connectors remain native; other database types use the generic JDBC bridge.
JDBC needs Java 11+ and vendor JAR files. Each vendor still needs live validation against its server.

The dialog has a searchable database sidebar and a **Drivers** section for adding, editing and removing
custom native-compatible or JDBC drivers. JDBC configuration includes class, dependency JAR paths,
URL template, default port and file target. Built-in drivers are copied into custom entries for editing.
Driver definitions persist in `drivers.toml`; saved sources protect their driver's protocol/target from
changes and block its removal. The app uses its own UI implementation. Upstream download coordinates and installation
instructions are recorded in [driver setup](jdbc-drivers.md).

The bridge keeps one JDBC connection per session, carries credentials over stdin, supports result limits,
metadata introspection and connection options for read-only/transactions/isolation where supported by
the driver. It searches only configured JAR paths and `<config-dir>/jdbc/<driver-id>/`.

Some drivers need extra fields, visible in their URL templates: SQL Server instance name, Oracle SID or
service name (plus Thin/OCI variants), MongoDB `authSource`, `+srv` and read preference, Snowflake
authenticator. The catalog records which driver needs which (`Extra`); the form has no fields for them yet.

## Not covered yet

- Password storage modes and the keychain.
- SSH: interactive credential prompts after rejected authentication, reading
  `~/.ssh/config`, certificate host keys, and jump hosts.
- The extra per-database fields above.
- Data source templates, "Data Source from URL / Path / Cloud", Dev Container environment, session templates.
- Editing or deleting an existing data source from the GUI.
- Enforcing the "Saved" options listed above.
