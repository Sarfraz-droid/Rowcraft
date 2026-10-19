//! The single place that knows about every dialect crate.
//! Each is behind a cargo feature named after its id, so slim builds are possible:
//! `cargo build -p rowcraft-registry --no-default-features --features pg,mysql,sqlite`.

use rowcraft_dialect::Registry;

pub fn default_registry() -> Registry {
    #[allow(unused_mut)]
    let mut registry = Registry::new();
    #[cfg(feature = "http")]
    {
        registry.register(rowcraft_dialect_http::HttpDialect::neo4j());
        registry.register(rowcraft_dialect_http::HttpDialect::graphql());
    }
    #[cfg(feature = "pg")]
    registry.register(rowcraft_dialect_pg::PgDialect);
    #[cfg(feature = "redshift")]
    registry.register(rowcraft_dialect_redshift::RedshiftDialect);
    #[cfg(feature = "greenplum")]
    registry.register(rowcraft_dialect_greenplum::GreenplumDialect);
    #[cfg(feature = "cockroach")]
    registry.register(rowcraft_dialect_cockroach::CockroachDialect);
    #[cfg(feature = "mysql")]
    registry.register(rowcraft_dialect_mysql::MysqlDialect);
    #[cfg(feature = "mariadb")]
    registry.register(rowcraft_dialect_mariadb::MariadbDialect);
    #[cfg(feature = "sqlite")]
    registry.register(rowcraft_dialect_sqlite::SqliteDialect);
    #[cfg(feature = "mssql")]
    registry.register(rowcraft_dialect_mssql::MssqlDialect);
    #[cfg(feature = "clickhouse")]
    registry.register(rowcraft_dialect_clickhouse::ClickhouseDialect);
    #[cfg(feature = "cassandra")]
    registry.register(rowcraft_dialect_cassandra::CassandraDialect);
    #[cfg(feature = "mongo")]
    registry.register(rowcraft_dialect_mongo::MongoDialect);
    #[cfg(feature = "redis")]
    registry.register(rowcraft_dialect_redis::RedisDialect);
    #[cfg(feature = "dynamo")]
    registry.register(rowcraft_dialect_dynamo::DynamoDialect);
    #[cfg(feature = "h2")]
    registry.register(rowcraft_dialect_h2::H2Dialect);
    #[cfg(feature = "snowflake")]
    registry.register(rowcraft_dialect_snowflake::SnowflakeDialect);
    #[cfg(feature = "bigquery")]
    registry.register(rowcraft_dialect_bigquery::BigqueryDialect);
    #[cfg(feature = "couchbase")]
    registry.register(rowcraft_dialect_couchbase::CouchbaseDialect);
    #[cfg(feature = "exasol")]
    registry.register(rowcraft_dialect_exasol::ExasolDialect);
    #[cfg(feature = "hive")]
    registry.register(rowcraft_dialect_hive::HiveDialect);
    #[cfg(feature = "spark")]
    registry.register(rowcraft_dialect_spark::SparkDialect);
    #[cfg(feature = "vertica")]
    registry.register(rowcraft_dialect_vertica::VerticaDialect);
    #[cfg(feature = "sybase")]
    registry.register(rowcraft_dialect_sybase::SybaseDialect);
    #[cfg(feature = "db2")]
    registry.register(rowcraft_dialect_db2::Db2Dialect);
    #[cfg(feature = "derby")]
    registry.register(rowcraft_dialect_derby::DerbyDialect);
    #[cfg(feature = "oracle")]
    registry.register(rowcraft_dialect_oracle::OracleDialect);
    #[cfg(feature = "hsqldb")]
    registry.register(rowcraft_dialect_hsqldb::HsqldbDialect);
    #[cfg(feature = "trino")]
    {
        registry.register(rowcraft_dialect_trino::TrinoDialect);
        registry.register(rowcraft_dialect_trino::PrestoDialect);
    }
    #[cfg(feature = "jdbc")]
    {
        registry.register(rowcraft_dialect_jdbc::JdbcDialect::default());
        // Native implementations take precedence; JDBC fills the remaining catalog entries.
        let pending: Vec<_> = registry
            .iter()
            .filter(|d| !d.connects())
            .map(|d| (d.id().0, d.display_name(), d.default_port()))
            .collect();
        for (id, name, port) in pending {
            registry.register(rowcraft_dialect_jdbc::JdbcDialect { id, name, port });
        }
    }
    registry
}

#[cfg(all(test, feature = "all"))]
mod tests {
    use super::*;

    #[test]
    fn registers_all_builtin_dialects() {
        // The 26 dedicated database dialects, plus Trino and Presto.
        assert_eq!(default_registry().len(), 31);
    }

    #[test]
    fn every_catalog_entry_has_an_execution_backend() {
        let registry = default_registry();
        for driver in rowcraft_drivers::all() {
            let dialect = registry.get(driver.dialect.expect("catalog driver dialect")).unwrap();
            assert!(dialect.connects(), "{} has no execution backend", driver.name);
        }
    }

    #[test]
    fn lookup_by_id() {
        let r = default_registry();
        assert_eq!(r.get("pg").map(|d| d.display_name()).ok(), Some("PostgreSQL"));
        assert!(r.get("nope").is_err());
    }
}
