<a id="readme-top"></a>

[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Stars][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]
[![CI][ci-shield]][ci-url]

<br />
<div align="center">
  <a href="https://github.com/Sarfraz-droid/Rowcraft">
    <img src="apps/rowcraft-desktop/public/brand/svg/icon-blue.svg" alt="Rowcraft logo" width="80" height="80">
  </a>

  <h1 align="center">Rowcraft</h1>

  <p align="center">
    An open-source database workbench. Rust engine. Tauri desktop. One shared CLI.
    <br />
    <a href="#documentation"><strong>Explore the docs »</strong></a>
    <br />
    <br />
    <a href="https://github.com/Sarfraz-droid/Rowcraft/releases">Releases</a>
    &middot;
    <a href="https://github.com/Sarfraz-droid/Rowcraft/issues/new?title=Bug%3A%20">Report a Bug</a>
    &middot;
    <a href="https://github.com/Sarfraz-droid/Rowcraft/issues/new?title=Feature%3A%20">Request a Feature</a>
  </p>
</div>

<details>
  <summary>Table of Contents</summary>
  <ol>
    <li><a href="#about-the-project">About the Project</a>
      <ul><li><a href="#built-with">Built With</a></li></ul>
    </li>
    <li><a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#prerequisites">Prerequisites</a></li>
        <li><a href="#installation">Installation</a></li>
      </ul>
    </li>
    <li><a href="#usage">Usage</a></li>
    <li><a href="#database-support">Database Support</a></li>
    <li><a href="#configuration-and-credentials">Configuration and Credentials</a></li>
    <li><a href="#build-installers">Build Installers</a></li>
    <li><a href="#development-and-testing">Development and Testing</a></li>
    <li><a href="#architecture">Architecture</a></li>
    <li><a href="#roadmap">Roadmap</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
    <li><a href="#documentation">Documentation</a></li>
    <li><a href="#acknowledgments">Acknowledgments</a></li>
  </ol>
</details>

## About the Project

[![Rowcraft desktop workbench][product-screenshot]](apps/landing/assets/ui.jpg)

Rowcraft is a desktop workspace for exploring databases, writing queries, and inspecting results. SQL databases, document stores, key-value stores, and HTTP APIs share a connection explorer, with consoles adapted to their query languages.

The desktop and command-line app share a Rust engine, so saved connections and database behavior stay consistent across both interfaces.

### Features

- **Database explorer:** browse schemas, tables, views, columns, indexes, keys, and routines where the connector exposes them.
- **Query consoles:** SQL, MongoDB shell statements, Redis commands, CQL, PartiQL, SQL++, Cypher, and GraphQL, depending on the selected source.
- **Results workspace:** virtualized tables, JSON views, and charts for supported result shapes.
- **Persistent sessions:** switch between tabs without reconnecting or rerunning existing results. Consoles for the same source share its backend session and transaction state.
- **Connection management:** saved sources, connection testing, schema filters, custom drivers, and SSH tunnels.
- **SQLite file import:** open a local database or import a validated copy through the desktop UI.
- **Optional AI assistance:** query help through a configurable gateway, with optional local 9router integration.
- **Headless CLI:** run queries, export results, inspect catalogs, and manage sources without starting the desktop app.

The desktop uses the system webview. Database operations run in Rust, and production frontend assets are embedded in the application.

### Project Status

Rowcraft is in early development. Connector coverage and validation vary by database; a driver appearing in the catalog does not mean every vendor, version, or feature has been tested. See the [support table](#database-support) and [connector boundaries](docs/database-consoles.md) before choosing a connection.

Rowcraft is an independently developed database workbench. Performance claims require measurements against equivalent workloads.

Database packages come directly from upstream suppliers and are kept in Rowcraft's own storage; the complete download list is maintained in [the driver catalog](drivers/catalog.json).

### Built With

| Layer | Technology |
| --- | --- |
| Database engine and CLI | [Rust](https://www.rust-lang.org/), Tokio, Clap |
| Desktop shell | [Tauri 2](https://tauri.app/) and the system webview |
| Interface | [React](https://react.dev/), TypeScript, Vite, Tailwind CSS |
| Query editor | [CodeMirror](https://codemirror.net/) |
| Result grid | TanStack Virtual |
| Database connectivity | Rust drivers, HTTP APIs, compatible wire protocols, JDBC |

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Getting Started

### Prerequisites

For desktop development, install:

| Tool | Requirement |
| --- | --- |
| Rust | Stable toolchain; see [rust-toolchain.toml](rust-toolchain.toml) |
| Node.js | 24, matching the installer workflow |
| pnpm | 11.6.0, pinned in the desktop package |
| Native build tools | Xcode command-line tools on macOS; Visual Studio C++ Build Tools and Windows SDK on Windows; WebKitGTK 4.1 and development libraries on Linux |

Follow the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/) for the complete native dependency list. Java 11+ and vendor driver JARs are needed only for JDBC connections. SQLite is bundled and needs no separate server.

### Installation

1. Clone the repository:

   ```sh
   git clone https://github.com/Sarfraz-droid/Rowcraft.git
   cd Rowcraft
   ```

2. Install frontend dependencies and start the desktop app:

   ```sh
   pnpm --dir apps/rowcraft-desktop install --frozen-lockfile
   pnpm --dir apps/rowcraft-desktop tauri dev
   ```

On macOS and Linux, the Makefile provides the same workflow:

```sh
make gui-install
make run-gui
```

To produce a distributable application, follow [Build Installers](#build-installers).

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Usage

### Connect and Query

1. Select **+ Add** and choose a database type.
2. Enter connection details and use **Test Connection**.
3. Save the source and select it in the explorer to connect.
4. Open a console and run a statement with **Cmd+Enter** on macOS or **Ctrl+Enter** elsewhere. A selected statement runs instead of the whole editor.

The default result limit is 1,000 rows. Use explicit server-side limits when exploring large datasets.

### Try SQLite without a server

With the `sqlite3` command installed, this creates a small disposable database and opens the desktop app with a separate configuration:

```sh
make desktop-demo
```

Demo data lives under `.dev/`. Running the command again recreates its database and configuration.

To use your own file, choose **+ Add → SQLite → Upload SQLite file…**. Rowcraft imports a copy into `<config-dir>/sqlite-imports/`; the upload limit is 256 MB. For larger files, enter the local database path directly.

### Preview the interface in a browser

```sh
make web
```

Open `http://localhost:1420`. This development preview uses mock data. Real connections, SQLite imports, and database queries require the desktop app.

### Command-line Interface

The CLI binary is named `rowcraft`. To build it:

```sh
cargo build --release -p rowcraft-cli
```

The executable is `target/release/rowcraft` on macOS/Linux and `target/release/rowcraft.exe` on Windows. The examples below use `cargo run` so no PATH setup is required.

#### Query a local SQLite database

```sh
cargo run -p rowcraft-cli -- ds add local-sqlite --dialect sqlite --database /absolute/path/to/example.sqlite
cargo run -p rowcraft-cli -- ds test local-sqlite
cargo run -p rowcraft-cli -- query -d local-sqlite "SELECT sqlite_version();"
cargo run -p rowcraft-cli -- introspect -d local-sqlite -f json
```

#### Connect to PostgreSQL

Set `APP_PW` in your shell to the database password, then save a reference to that environment variable:

```sh
cargo run -p rowcraft-cli -- ds add local-pg \
  --host localhost --port 5432 --database app --user me --password-env APP_PW

cargo run -p rowcraft-cli -- ds test local-pg
cargo run -p rowcraft-cli -- query -d local-pg -f csv "SELECT * FROM people LIMIT 1000;" > people.csv
cargo run -p rowcraft-cli -- introspect -d local-pg -s public
```

The desktop app reads the same saved sources. When a source uses an environment-variable password, launch the desktop app from a shell where that variable is set.

#### Discover commands and drivers

```sh
cargo run -p rowcraft-cli -- --help
cargo run -p rowcraft-cli -- dialects
cargo run -p rowcraft-cli -- drivers
cargo run -p rowcraft-cli -- ds --help
cargo run -p rowcraft-cli -- query --help
```

CLI exit codes are `0` for success, `1` for statement errors, `2` for connection/authentication errors, and `3` for usage/configuration errors.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Database Support

Rowcraft uses Rust drivers, HTTP connectors, compatible wire protocols, and a JDBC bridge. The table records the validation described in this repository; it is not a guarantee for every deployment.

| Database or family | Connection path | Validation / limitations |
| --- | --- | --- |
| PostgreSQL, YugabyteDB | PostgreSQL driver | Live-server verification recorded |
| SQLite | Bundled SQLite | Local-file round trips verified |
| MySQL, MariaDB; Aurora MySQL, TiDB, SingleStore via MySQL | MySQL driver | MySQL 9.7 verified; other variants need live validation |
| Redshift, Greenplum, CockroachDB | PostgreSQL protocol | Greenplum path tested against PostgreSQL 14; vendor validation remains |
| Vertica | PostgreSQL protocol | No live verification; authentication support is limited |
| H2 | PostgreSQL server mode | Requires H2 PG server mode; no live verification |
| MongoDB, DocumentDB | MongoDB driver | MongoDB 7 verified; supported shell statements only |
| Redis | Redis driver | Live-server verification recorded |
| Cassandra | Scylla/Cassandra driver | Cassandra 5 verified |
| DynamoDB | PartiQL over HTTP | DynamoDB Local verified |
| Trino, Presto | HTTP statement protocol | Trino verified; Presto needs live validation |
| ClickHouse | HTTP | Request-format tests with a fake server |
| SQL Server, Azure SQL, Synapse | TDS driver | SQL authentication; unit tests only |
| Snowflake | REST API | Password authentication; unit tests only |
| Couchbase | Query service HTTP API | Unit tests only |
| Neo4j, GraphQL APIs | HTTP | Unit tests; live-server verification pending |
| Oracle, Db2, Derby, HSQLDB, Exasol, Hive, Spark, BigQuery, Athena, DuckDB, Databricks, Firebird, Informix, HANA, Teradata, and other catalog entries | JDBC | Java 11+, vendor JARs, and a reachable database required; coverage varies |

For native query languages, endpoint configuration, introspection behavior, and unsupported features, read [database consoles and support boundaries](docs/database-consoles.md). Connection options that are implemented versus only stored are documented in [data source options](docs/data-source-options.md).

### JDBC and custom drivers

Create a driver under **+ Add → Drivers → + Custom driver**. A JDBC driver needs its JARs, a driver class when auto-discovery is unavailable, and a URL template. Templates support `{host}`, `{port}`, `{database}`, and `{user}`.

For built-in JDBC entries, place JARs under `<config-dir>/jdbc/<driver-id>/` or configure `jdbc.jars`, `jdbc.class`, and `jdbc.url_template` in Advanced. Driver downloads and their pinned upstream coordinates are maintained in [our driver catalog](drivers/catalog.json). Run `python3 scripts/download-drivers.py --all` to install the available JDBC packages directly from Maven Central. See [driver setup](docs/jdbc-drivers.md) for vendor-only packages and Java setup. The app never searches another application for drivers or Java.

Use `ROWCRAFT_JAVA` to select a Java executable or set `JAVA_HOME`. See [JDBC setup](docs/jdbc-drivers.md) for details.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Configuration and Credentials

Desktop and CLI sources share the same configuration directory:

| Platform | Default directory |
| --- | --- |
| macOS / Linux | `~/.config/rowcraft/` |
| Windows | `%APPDATA%\rowcraft\` |

`XDG_CONFIG_HOME` overrides the default base directory when set. `ROWCRAFT_CONFIG_DIR` overrides the entire configuration path on every platform.

Sources live in `datasources.toml`, custom drivers in `drivers.toml`, and shared SSH configurations in `ssh.toml`. Database passwords can be resolved from environment variables; desktop credential storage also uses references to the operating system credential store.

TLS is enabled by default for non-loopback SQL connections through the default connection policy. Connector-specific rules apply: for example, MongoDB Auto follows MongoDB URI defaults. Use verified TLS and database-side permissions appropriate to your environment; see the [connection options](docs/data-source-options.md).

SSH tunnels support password, key-pair, and agent authentication through Rust, without requiring an external `ssh` command. Unknown hosts are refused by default; `accept-new` can explicitly trust new hosts, while changed host keys remain rejected.

### Optional AI setup

Open **Settings → AI** to configure a gateway and model. The optional local 9router setup requires Node.js 22+ with npm and internet access on first start. The app can also connect to an existing compatible gateway.

AI requests can include conversation content, schema metadata, and sampled results when database tools are used. Choose a provider and data-access policy suitable for your database. Automatic query execution is restricted to supported read-only SQL backends; a supported console language does not automatically enable AI execution for that connector.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Build Installers

Build a release executable without packaging:

```sh
make gui
```

Build installers for your current operating system:

```sh
pnpm --dir apps/rowcraft-desktop installer
# Or, on macOS/Linux:
make installer
```

Builds run the frontend production build and embed its assets. Output goes under `target/release/bundle/`, or `target/<triple>/release/bundle/` when a target is specified.

| Platform | Configured outputs |
| --- | --- |
| macOS Apple Silicon / Intel | `.app`, `.dmg` |
| Windows x64 | NSIS `.exe` |
| Linux x64 | `.AppImage`, `.deb`, `.rpm` |

The [Desktop installers workflow](.github/workflows/installers.yml) builds all four OS/architecture combinations on a manual run. Pushing an `v<VERSION>` tag creates a draft GitHub release. macOS builds use ad-hoc signing unless distribution credentials are provided; Windows signing is not configured by default.

See [installer builds and signing](docs/installers.md) for native dependencies, versioning, signing secrets, and release steps. See [package distribution](docs/package-distribution.md) for website downloads and package-manager publishing.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Development and Testing

| Command | Purpose |
| --- | --- |
| `make build` | Build the Rust workspace excluding the desktop shell |
| `make run-gui` | Run the desktop app with frontend hot reload |
| `make web` | Run the mock browser preview |
| `make test` | Run Rust tests excluding the desktop shell |
| `make lint` | Run Clippy with warnings treated as errors |
| `make fmt` | Format Rust source |
| `make test-desktop` | Type-check the frontend, test UI helpers and services, and check the desktop shell |
| `make test-live` | Start the disposable PostgreSQL instance and run live-enabled tests |
| `make test-ssh` | Start the disposable SSH fixture and run tunnel tests |
| `make landing` | Serve the static landing page on port 4321 |
| `make landing-check` | Check landing-page JavaScript and run its tests |

The PostgreSQL fixture needs local PostgreSQL tools; the SSH fixture needs OpenSSH server tools. Both use `.dev/`. Live PostgreSQL tests are skipped unless `ROWCRAFT_TEST_PG_PORT` is configured; `make test-live` configures it for the fixture.

Direct commands are useful on Windows or when working on one part of the project:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --exclude rowcraft-desktop --all-targets -- -D warnings
cargo test --workspace --exclude rowcraft-desktop
pnpm --dir apps/rowcraft-desktop typecheck
pnpm --dir apps/rowcraft-desktop test
cargo test -p rowcraft-ui-model
cargo check -p rowcraft-desktop
```

The [CI workflow](.github/workflows/ci.yml) defines the checks run on pushes to `main` and pull requests.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Architecture

The desktop and CLI share the Rust database engine. The Tauri shell exposes commands to React through IPC; reusable workbench logic lives in `rowcraft-ui-model`. Database-specific behavior is implemented behind the `rowcraft-dialect` traits.

```text
apps/rowcraft-desktop/src          React, TypeScript, CodeMirror, result views
           │ Tauri IPC
apps/rowcraft-desktop/src-tauri    Desktop shell and command bridge
           │
crates/rowcraft-ui-model           Workbench services and validation
           │
apps/rowcraft-cli ──────────────── Shared Rust engine and configuration
                            │
                         dialects/* → Drivers, HTTP APIs, JDBC, wire/*
```

| Path | Contents |
| --- | --- |
| `apps/rowcraft-desktop/` | Desktop frontend and Tauri shell |
| `apps/rowcraft-cli/` | The `rowcraft` command-line application |
| `apps/landing/` | Static product website and download configuration |
| `crates/` | Shared engine, configuration, catalog models, SQL helpers, I/O, and SSH |
| `dialects/` | Database connector implementations and shared protocol bases |
| `wire/` | Protocol implementations for connectors without an existing Rust driver |
| `docs/` | Architecture, database setup, and distribution guides |

Read the [architecture guide](docs/architecture.md) for dependency rules and the [design specification](SPEC.md) for project direction. Planned behavior in the specification should not be treated as an implemented feature.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Roadmap

The [project specification](SPEC.md) describes the wider design. Current areas for follow-up include:

- [ ] Expand live-server coverage for connectors currently verified only with mocks or unit tests.
- [ ] Validate more database vendor/version combinations and document compatibility.
- [ ] Profile startup, CPU, and memory against repeatable workloads.
- [ ] Complete public release signing, notarization, and package-manager distribution.
- [ ] Add the standalone license texts declared by the workspace metadata.

These are development goals, not release-date commitments. Use [open issues](https://github.com/Sarfraz-droid/Rowcraft/issues) to propose work and track discussions. Check the support table for what is available today.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Contributing

Contributions are welcome across connectors, desktop UX, testing, documentation, and packaging.

1. For substantial changes, open an issue describing the problem and proposed scope before starting.
2. Keep changes focused. Include a reproducible example for bug fixes and document any user-visible behavior changes.
3. Follow the existing module boundaries. Shared engine code should remain independent of the UI; connector changes belong in the appropriate dialect crate.
4. Run formatting, linting, and tests relevant to the change. Add meaningful coverage for new behavior and record which checks passed.
5. Open a pull request explaining the problem, the change, and validation. For connector work, state the database/version tested and distinguish live-server tests from mocks.

When reporting a bug, include your OS and architecture, Rowcraft version or commit, database and driver versions, steps to reproduce, and redacted logs or error output. Remove passwords, tokens, connection strings containing credentials, and private data before sharing.

### Pull Request Workflow

1. Fork the repository and clone your fork.
2. Create a branch for the change:

   ```sh
   git switch -c feature/describe-your-change
   ```

3. Implement the change and run the relevant checks above.
4. Commit and push to your fork:

   ```sh
   git add <changed-files>
   git commit -m "Describe the change"
   git push -u origin feature/describe-your-change
   ```

5. Open a pull request against Rowcraft and include your validation results.

Browse [contributors](https://github.com/Sarfraz-droid/Rowcraft/graphs/contributors) or [open pull requests](https://github.com/Sarfraz-droid/Rowcraft/pulls) to see ongoing work.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## License

The Cargo workspace declares `MIT OR Apache-2.0`. Standalone license texts have not yet been added to this repository. Bundled fonts and other third-party assets retain their own license notices.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Contact

Maintainer: [Sarfraz-droid](https://github.com/Sarfraz-droid)

Project: [Sarfraz-droid/Rowcraft](https://github.com/Sarfraz-droid/Rowcraft)

For bugs and feature ideas, [open an issue](https://github.com/Sarfraz-droid/Rowcraft/issues/new). Include enough detail to reproduce the problem, and remove credentials and private data from reports.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Documentation

- [Architecture and dependency rules](docs/architecture.md)
- [Product specification](SPEC.md)
- [Desktop visual system](DESIGN.md)
- [Database consoles and support boundaries](docs/database-consoles.md)
- [Data source options](docs/data-source-options.md)
- [JDBC drivers](docs/jdbc-drivers.md)
- [Installer builds and signing](docs/installers.md)
- [Package distribution](docs/package-distribution.md)
- [Landing-page development](apps/landing/README.md)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Acknowledgments

- [Best-README-Template](https://github.com/othneildrew/Best-README-Template) by othneildrew for this README's structure.
- The Rust, Tauri, React, and CodeMirror communities, and the maintainers of the database drivers used by Rowcraft.
- [Shields.io](https://shields.io/) for repository badges.
- The creators of bundled fonts and other assets; their license notices remain alongside those assets.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- Reference links and images -->
[contributors-shield]: https://img.shields.io/github/contributors/Sarfraz-droid/Rowcraft.svg?style=for-the-badge
[contributors-url]: https://github.com/Sarfraz-droid/Rowcraft/graphs/contributors
[forks-shield]: https://img.shields.io/github/forks/Sarfraz-droid/Rowcraft.svg?style=for-the-badge
[forks-url]: https://github.com/Sarfraz-droid/Rowcraft/forks
[stars-shield]: https://img.shields.io/github/stars/Sarfraz-droid/Rowcraft.svg?style=for-the-badge
[stars-url]: https://github.com/Sarfraz-droid/Rowcraft/stargazers
[issues-shield]: https://img.shields.io/github/issues/Sarfraz-droid/Rowcraft.svg?style=for-the-badge
[issues-url]: https://github.com/Sarfraz-droid/Rowcraft/issues
[ci-shield]: https://img.shields.io/github/actions/workflow/status/Sarfraz-droid/Rowcraft/ci.yml?branch=main&style=for-the-badge&label=CI
[ci-url]: https://github.com/Sarfraz-droid/Rowcraft/actions/workflows/ci.yml
[product-screenshot]: apps/landing/assets/ui.jpg
