//! Rowcraft's built-in database catalog, stored in this repository.
//! `dialect` selects a native implementation or the generic JDBC bridge.
//! Upstream driver downloads are recorded in `drivers/catalog.json`.

use std::sync::{LazyLock, RwLock};

/// How the connection target is addressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Host and port.
    Network,
    /// A file or directory path (embedded databases).
    File,
}

/// Extra connection fields a database needs beyond host, port, database, user and password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extra {
    None,
    /// SQL Server named instance (`host\instance`).
    Instance,
    /// Oracle: a SID or a service name selects the database.
    OracleSidOrService,
    /// MongoDB: authentication database and `+srv` hosts.
    MongoAuth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Driver {
    pub id: &'static str,
    /// Database display name.
    pub name: &'static str,
    /// Execution dialect (`Dialect::id()`), including `jdbc` for generic drivers.
    pub dialect: Option<&'static str>,
    pub port: Option<u16>,
    pub target: Target,
    pub extra: Extra,
    /// Defined by the user (`drivers.toml`) rather than built in.
    pub custom: bool,
}

const fn net(
    id: &'static str,
    name: &'static str,
    dialect: Option<&'static str>,
    port: Option<u16>,
) -> Driver {
    Driver { id, name, dialect, port, target: Target::Network, extra: Extra::None, custom: false }
}

const fn file(id: &'static str, name: &'static str, dialect: Option<&'static str>) -> Driver {
    Driver {
        id,
        name,
        dialect,
        port: None,
        target: Target::File,
        extra: Extra::None,
        custom: false,
    }
}

const fn with(d: Driver, extra: Extra) -> Driver {
    Driver { extra, ..d }
}

static ENTRIES: &[Driver] = &[
    net("athena", "Amazon Athena", Some("jdbc"), None),
    net("aurora_mysql", "Amazon Aurora MySQL", Some("mysql"), Some(3306)),
    net("documentdb", "Amazon DocumentDB", Some("mongo"), Some(27017)),
    net("dynamodb", "Amazon DynamoDB", Some("dynamo"), None),
    net("redshift", "Amazon Redshift", Some("redshift"), Some(5439)),
    net("cassandra", "Apache Cassandra", Some("cassandra"), Some(9042)),
    file("derby_embedded", "Apache Derby (Embedded)", Some("derby")),
    net("derby_remote", "Apache Derby (Remote)", Some("derby"), Some(1527)),
    net("hive", "Apache Hive", Some("hive"), Some(10000)),
    net("ignite", "Apache Ignite", Some("jdbc"), None),
    net("phoenix_thick", "Apache Phoenix", Some("jdbc"), None),
    net("phoenix_thin", "Apache Phoenix Thin", Some("jdbc"), Some(8765)),
    net("spark", "Apache Spark", Some("spark"), Some(10000)),
    net("azure_sql", "Azure SQL Database", Some("mssql"), Some(1433)),
    net("synapse", "Azure Synapse Analytics", Some("mssql"), Some(1433)),
    net("clickhouse", "ClickHouse", Some("clickhouse"), Some(8123)),
    net("cockroach", "CockroachDB", Some("cockroach"), Some(26257)),
    net("couchbase", "Couchbase Server", Some("couchbase"), Some(8093)),
    net("databricks", "Databricks", Some("jdbc"), Some(443)),
    net("denodo", "Denodo", Some("jdbc"), Some(9999)),
    file("duckdb", "DuckDB", Some("jdbc")),
    net("elasticsearch", "Elasticsearch", Some("jdbc"), Some(9200)),
    net("exasol", "Exasol", Some("exasol"), None),
    net("firebird", "Firebird", Some("jdbc"), Some(3050)),
    net("cloudspanner", "Google Cloud Spanner", Some("jdbc"), Some(9200)),
    net("bigquery", "Google BigQuery", Some("bigquery"), Some(443)),
    net("greenplum", "Greenplum", Some("greenplum"), Some(5432)),
    // 5435 is H2's PostgreSQL-mode server; 9092 is the H2 TCP protocol, not implemented.
    net("h2", "H2", Some("h2"), Some(5435)),
    file("hsqldb_local", "HSQLDB (Local)", Some("hsqldb")),
    net("hsqldb_remote", "HSQLDB (Remote)", Some("hsqldb"), Some(9001)),
    net("db2", "IBM Db2", Some("db2"), Some(50000)),
    net("db2_jtopen", "IBM Db2 for i (JTOpen)", Some("db2"), Some(446)),
    net("informix", "IBM Informix", Some("jdbc"), Some(1526)),
    net("influxdb", "InfluxDB", Some("jdbc"), Some(8181)),
    net("iris", "InterSystems IRIS", Some("jdbc"), Some(9091)),
    net("mariadb", "MariaDB", Some("mariadb"), Some(3306)),
    net("mimer", "Mimer SQL", Some("jdbc"), Some(1360)),
    with(net("sqlserver", "Microsoft SQL Server", Some("mssql"), Some(1433)), Extra::Instance),
    with(net("mongodb", "MongoDB", Some("mongo"), Some(27017)), Extra::MongoAuth),
    net("neo4j", "Neo4j (HTTP Query API)", Some("neo4j"), Some(7474)),
    net("graphql", "GraphQL (HTTP API)", Some("graphql"), Some(4000)),
    net("mysql", "MySQL", Some("mysql"), Some(3306)),
    // ODBC requires a separately installed JDBC/ODBC bridge and native ODBC driver.
    net("odbc", "ODBC", Some("jdbc"), None),
    net("openedge", "OpenEdge", Some("jdbc"), None),
    with(net("oracle", "Oracle", Some("oracle"), Some(1521)), Extra::OracleSidOrService),
    net("postgresql", "PostgreSQL", Some("pg"), Some(5432)),
    net("presto", "Presto", Some("presto"), Some(8080)),
    net("redis", "Redis", Some("redis"), Some(6379)),
    net("hana", "SAP HANA", Some("jdbc"), None),
    net("singlestore", "SingleStore", Some("mysql"), Some(3306)),
    file("sqlite", "SQLite", Some("sqlite")),
    net("snowflake", "Snowflake", Some("snowflake"), Some(443)),
    net("sybase", "Sybase ASE", Some("sybase"), Some(5000)),
    net("tarantool", "Tarantool", Some("jdbc"), Some(3301)),
    net("teradata", "Teradata Vantage", Some("jdbc"), None),
    net("tibero", "Tibero", Some("jdbc"), Some(8629)),
    net("tidb", "TiDB", Some("mysql"), Some(3306)),
    net("trino", "Trino", Some("trino"), Some(8080)),
    net("vertica", "Vertica", Some("vertica"), Some(5433)),
    net("yugabyte", "YugabyteDB", Some("pg"), Some(5433)),
];

static SORTED: LazyLock<Vec<Driver>> = LazyLock::new(|| {
    let mut all = ENTRIES.to_vec();
    all.sort_by_key(|d| d.name.to_lowercase());
    all
});

/// A driver the user defined: a name, default port and target kind on top of one of our dialects.
/// Select `jdbc` to load arbitrary JDBC JARs through the Java subprocess bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomDriver {
    pub id: String,
    pub name: String,
    /// Our dialect id (`Dialect::id()`); whether it exists in this build is checked by the caller.
    pub dialect: String,
    pub port: Option<u16>,
    pub target: Target,
}

impl CustomDriver {
    /// Checks the fields in isolation; clashes with other entries are checked by [`set_custom`].
    pub fn validate(&self) -> Result<(), String> {
        let id = &self.id;
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(format!(
                "driver id `{id}` must be non-empty and use only a-z, 0-9, `_` and `-`"
            ));
        }
        if self.name.trim().is_empty() {
            return Err(format!("driver `{id}` needs a display name"));
        }
        if self.dialect.trim().is_empty() {
            return Err(format!("driver `{id}` needs a dialect"));
        }
        if self.port == Some(0) {
            return Err(format!("driver `{id}`: port 0 is not valid"));
        }
        if self.target == Target::File && self.port.is_some() {
            return Err(format!("driver `{id}`: a file-based driver has no port"));
        }
        Ok(())
    }
}

/// The merged catalog once custom drivers are set; `None` means built-ins only.
static CURRENT: RwLock<Option<&'static [Driver]>> = RwLock::new(None);

/// Merges `custom` with the built-ins, sorted by display name. Rejects an entry that is invalid,
/// reuses a built-in id, or repeats another custom id; the rest are kept. Returns the catalog and
/// one message per rejected entry.
fn merge(custom: Vec<CustomDriver>) -> (Vec<Driver>, Vec<String>) {
    let mut all = ENTRIES.to_vec();
    let mut rejected = Vec::new();
    for c in custom {
        if let Err(e) = c.validate() {
            rejected.push(e);
        } else if all.iter().any(|d| d.id == c.id) {
            rejected.push(format!("driver id `{}` is already taken", c.id));
        } else {
            // Leaked on purpose: `Driver` holds `&'static str` and the set changes at most a few
            // times per process.
            let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
            all.push(Driver {
                id: leak(c.id),
                name: leak(c.name.trim().to_owned()),
                dialect: Some(leak(c.dialect.trim().to_owned())),
                port: c.port,
                target: c.target,
                extra: Extra::None,
                custom: true,
            });
        }
    }
    all.sort_by_key(|d| d.name.to_lowercase());
    (all, rejected)
}

/// Replaces the set of custom drivers. Call it at startup and after the user's list changes.
/// Returns one message per rejected entry.
pub fn set_custom(custom: Vec<CustomDriver>) -> Vec<String> {
    let (merged, rejected) = merge(custom);
    let merged: &'static [Driver] = Box::leak(merged.into_boxed_slice());
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = Some(merged);
    rejected
}

/// Every entry, sorted by display name.
pub fn all() -> &'static [Driver] {
    match *CURRENT.read().unwrap_or_else(|e| e.into_inner()) {
        Some(merged) => merged,
        None => &SORTED,
    }
}

pub fn find(id: &str) -> Option<&'static Driver> {
    all().iter().find(|d| d.id == id)
}

/// The driver to preselect for a dialect id: the first catalog entry using it.
pub fn default_for_dialect(dialect: &str) -> Option<&'static Driver> {
    all().iter().find(|d| d.dialect == Some(dialect))
}

/// Entries whose display name contains `query`, case-insensitively. An empty query matches all.
pub fn search(query: &str) -> Vec<&'static Driver> {
    let q = query.trim().to_lowercase();
    all().iter().filter(|d| d.name.to_lowercase().contains(&q)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_names_sorted() {
        let catalog = all(); // one snapshot: other tests may register custom drivers
        let mut ids: Vec<_> = catalog.iter().map(|d| d.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), catalog.len());
        let names: Vec<_> = catalog.iter().map(|d| d.name.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "all() sorts by display name");
    }

    #[test]
    fn file_databases_have_no_port() {
        for d in all().iter().filter(|d| d.target == Target::File) {
            assert_eq!(d.port, None, "{}", d.name);
        }
    }

    #[test]
    fn every_dialect_in_the_spec_has_a_driver() {
        // The 26 dedicated database dialects (SPEC §1.2) are all reachable from the catalog.
        for dialect in [
            "pg",
            "redshift",
            "greenplum",
            "cockroach",
            "mysql",
            "mariadb",
            "sqlite",
            "mssql",
            "sybase",
            "oracle",
            "db2",
            "derby",
            "h2",
            "hsqldb",
            "clickhouse",
            "vertica",
            "exasol",
            "snowflake",
            "bigquery",
            "hive",
            "spark",
            "cassandra",
            "mongo",
            "redis",
            "couchbase",
            "dynamo",
        ] {
            assert!(default_for_dialect(dialect).is_some(), "{dialect}");
        }
    }

    fn custom(id: &str) -> CustomDriver {
        CustomDriver {
            id: id.into(),
            name: "My Questy DB".into(),
            dialect: "pg".into(),
            port: Some(8812),
            target: Target::Network,
        }
    }

    #[test]
    fn custom_drivers_merge_sorted_and_flagged() {
        let (merged, rejected) = merge(vec![custom("questy")]);
        assert!(rejected.is_empty(), "{rejected:?}");
        assert_eq!(merged.len(), ENTRIES.len() + 1);
        let d = merged.iter().find(|d| d.id == "questy").unwrap();
        assert!(d.custom && d.dialect == Some("pg") && d.port == Some(8812));
        assert!(ENTRIES.iter().all(|d| !d.custom));
        let names: Vec<_> = merged.iter().map(|d| d.name.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn bad_custom_drivers_are_rejected_one_by_one() {
        let mut file = custom("f");
        file.target = Target::File;
        let (merged, rejected) = merge(vec![
            custom("postgresql"),
            custom("Bad Id"),
            CustomDriver { name: " ".into(), ..custom("noname") },
            CustomDriver { dialect: "".into(), ..custom("nodialect") },
            CustomDriver { port: Some(0), ..custom("port0") },
            file,
            custom("dup"),
            custom("dup"),
        ]);
        assert_eq!(rejected.len(), 7, "{rejected:?}");
        assert!(rejected[0].contains("already taken"));
        assert!(rejected[1].contains("a-z"));
        assert!(rejected[2].contains("display name"));
        assert!(rejected[3].contains("dialect"));
        assert!(rejected[4].contains("port 0"));
        assert!(rejected[5].contains("no port"));
        assert!(rejected[6].contains("already taken"));
        assert_eq!(merged.iter().filter(|d| d.custom).count(), 1, "only the first `dup`");
    }

    #[test]
    fn set_custom_makes_the_driver_findable() {
        assert!(find("unit-test-driver").is_none());
        assert!(set_custom(vec![custom("unit-test-driver")]).is_empty());
        let d = find("unit-test-driver").unwrap();
        assert!(d.custom);
        assert!(search("questy").iter().any(|d| d.id == "unit-test-driver"));
        assert_eq!(find("postgresql").map(|d| d.custom), Some(false));
    }

    #[test]
    fn lookups() {
        assert_eq!(find("postgresql").and_then(|d| d.port), Some(5432));
        assert_eq!(default_for_dialect("pg").map(|d| d.name), Some("PostgreSQL"));
        assert!(find("nope").is_none());
        let hits: Vec<_> = search(" sql ").iter().map(|d| d.id).collect();
        assert!(
            hits.contains(&"mysql") && hits.contains(&"postgresql") && !hits.contains(&"redis")
        );
        // `>=`: another test may add a custom driver between the two calls.
        assert!(search("").len() >= ENTRIES.len());
    }
}
