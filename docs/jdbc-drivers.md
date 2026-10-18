# JDBC and ODBC drivers

Native database connectors run directly in Rust. The remaining database families use an isolated Java 11+ subprocess with vendor JDBC drivers. Driver files are not bundled.

Create a custom JDBC driver in Drivers and enter its class, JAR files, and URL template. Templates support `{host}`, `{port}`, `{database}`, and `{user}`. Enter passwords through the data source password setting. For built-in families, Advanced properties `jdbc.class`, `jdbc.jars`, and `jdbc.url_template` override the defaults. `jdbc.jars` uses the operating system's path-list separator.

Use **Upload JAR files…** to select one or more driver and dependency JARs (up to 256 MB each). The app copies them into persistent `jdbc-imports` storage beside the data source configuration and adds their absolute paths to Driver files. Click **Save Driver** to save the configuration. You can also enter local JAR paths manually. Uploaded copies remain independent of the original files.

Built-in families also look for JAR files in the config directory under `jdbc/<driver-id>`. On macOS, downloaded drivers from an installed DataGrip profile can be reused. Set `DGLITE_JAVA` to select a runtime explicitly; otherwise a compatible `JAVA_HOME`, DataGrip's bundled runtime, or `java` on PATH is used.

ODBC uses the `jdbc:odbc:{database}` template, where Database contains the ODBC data source name. It requires a third-party JDBC/ODBC bridge JAR and the native ODBC driver. Modern JDKs do not include the old JDBC/ODBC bridge. Supply the bridge class and JARs in Advanced properties or create a custom JDBC driver using the bridge vendor's URL format.

Some vendors require a port or additional URL settings without a universal default. Enter their port or provide a complete JDBC URL; incomplete default URLs are rejected before connecting.
