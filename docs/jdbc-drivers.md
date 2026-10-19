# JDBC and ODBC drivers

Native database connectors run directly in Rust. The remaining database families use an isolated Java 11+ subprocess with vendor JDBC drivers. Driver files are not bundled.

The complete app-owned database list is checked into [`drivers/catalog.json`](../drivers/catalog.json).
It records native connectors, pinned Maven coordinates, and manual vendor downloads.
Release lists fetched directly from Maven Central are saved in `drivers/upstream/`; runtime
startup does not fetch or read another application's catalog.

In **Data Sources and Drivers**, select a database and click **Download driver** in
the General tab. The same action is available when configuring a built-in driver
in the Drivers view. Rowcraft downloads the pinned driver and its runtime dependencies
directly from Maven Central, verifies their sizes and SHA-256 checksums, and installs
them in its own storage. Progress appears beside the action; failed downloads can be
retried. No Python or Maven installation is needed for UI downloads. Install a standalone
Java runtime to use JDBC connections.

Already installed packages show their version and a **Download again** action.
Open **Settings → Drivers** to see downloaded packages, versions, storage use,
and individual JAR files. Expand a package to download it again or repair it;
**Remove** asks for confirmation before deleting its downloaded files. Connection
settings and manually uploaded JARs are kept. The same page lists built-in native
connectors, including PostgreSQL, separately; those are compiled into the app and
need no JDBC download. You can also download another JDBC package from Settings.
Vendor-only packages show **Get driver from vendor** instead; download those files
from the supplier and use **Upload JAR files…** in driver configuration.

For terminal installation, install Apache Maven and an independent JDK, then run:

```sh
python3 scripts/download-drivers.py --list
python3 scripts/download-drivers.py --all
# Or select individual databases:
python3 scripts/download-drivers.py duckdb oracle
```

The installer saves each package and its runtime dependencies under the app's own
`<config-dir>/jdbc/<driver-id>/` directory. It follows `ROWCRAFT_CONFIG_DIR`, `XDG_CONFIG_HOME`,
and the platform defaults used by the application. `--directory` changes the download
destination; use the app's config directory for automatic discovery. Maven downloads go
through Maven Central with strict checksum verification; each installed package also has
a SHA-256 receipt. Existing packages are replaced only after resolution succeeds.
Vendor-only entries print their download page and require manual installation.

The desktop app embeds `drivers/downloads.lock.json`, containing complete resolved
classpaths and checksums, alongside `drivers/catalog.json`. After updating Maven
coordinates in the catalog, maintainers run `scripts/download-drivers.py --all` and
then `python3 scripts/lock-driver-downloads.py` to refresh the lock file before building.

The bridge needs Java 11+, but each vendor package can require a newer Java release
(for example, the pinned Derby package requires Java 21+). Select a suitable standalone
JDK using `ROWCRAFT_JAVA` or `JAVA_HOME`. Downloaded JDBC files retain their upstream licenses;
installing them locally does not add them to the application's distributable bundle.

Create a custom JDBC driver in Drivers and enter its class, JAR files, and URL template. Templates support `{host}`, `{port}`, `{database}`, and `{user}`. Enter passwords through the data source password setting. For built-in families, Advanced properties `jdbc.class`, `jdbc.jars`, and `jdbc.url_template` override the defaults. `jdbc.jars` uses the operating system's path-list separator.

Use **Upload JAR files…** to select one or more driver and dependency JARs (up to 256 MB each). The app copies them into persistent `jdbc-imports` storage beside the data source configuration and adds their absolute paths to Driver files. Click **Save Driver** to save the configuration. You can also enter local JAR paths manually. Uploaded copies remain independent of the original files.

Built-in families also look for JAR files in the config directory under `jdbc/<driver-id>`. Set `ROWCRAFT_JAVA` to select a runtime explicitly; otherwise a compatible `JAVA_HOME` or `java` on PATH is used.

ODBC uses the `jdbc:odbc:{database}` template, where Database contains the ODBC data source name. It requires a third-party JDBC/ODBC bridge JAR and the native ODBC driver. Modern JDKs do not include the old JDBC/ODBC bridge. Supply the bridge class and JARs in Advanced properties or create a custom JDBC driver using the bridge vendor's URL format.

Some vendors require a port or additional URL settings without a universal default. Enter their port or provide a complete JDBC URL; incomplete default URLs are rejected before connecting.
