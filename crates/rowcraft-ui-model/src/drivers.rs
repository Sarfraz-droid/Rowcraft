//! The driver catalog as the "Add data source" dialog lists it.

use rowcraft_drivers::Target;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DriverInfo {
    pub dialect: Option<&'static str>,
    pub custom: bool,
    pub jdbc_url_template: Option<String>,
    pub id: &'static str,
    pub name: &'static str,
    /// Default port, shown as the Port placeholder.
    pub port: Option<u16>,
    /// A file database: the Database field holds a path and there is no host or port.
    pub file: bool,
    /// Why this entry cannot be used yet, if it cannot.
    pub caveat: Option<&'static str>,
}

/// Drivers whose name contains `query`. `ready` are the dialect ids whose `connect` works today.
pub fn list_drivers(query: &str, ready: &[&str]) -> Vec<DriverInfo> {
    rowcraft_drivers::search(query)
        .into_iter()
        .map(|d| DriverInfo {
            dialect: d.dialect,
            custom: d.custom,
            jdbc_url_template: rowcraft_dialect_jdbc::connection_url(
                &rowcraft_config::DataSourceConfig {
                    driver: Some(d.id.into()),
                    host: Some("{host}".into()),
                    port: Some(65535),
                    database: Some("{database}".into()),
                    user: Some("{user}".into()),
                    ..Default::default()
                },
                None,
            )
            .ok()
            .map(|url| url.replace("65535", "{port}")),
            id: d.id,
            name: d.name,
            port: d.port,
            file: d.target == Target::File,
            caveat: match d.dialect {
                None => Some("no Rust driver yet"),
                Some(dialect) if !ready.contains(&dialect) => Some("not implemented yet"),
                Some(_) => None,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_and_unready_drivers_say_why() {
        let all = list_drivers("", &["pg"]);
        let by_id = |id: &str| all.iter().find(|d| d.id == id).unwrap();
        assert_eq!(by_id("postgresql").caveat, None);
        assert_eq!(by_id("mysql").caveat, Some("not implemented yet"));
        assert_eq!(by_id("trino").caveat, Some("not implemented yet"));
        assert!(by_id("sqlite").file && by_id("sqlite").port.is_none());
    }

    #[test]
    fn search_is_a_case_insensitive_name_match() {
        let found = list_drivers("POSTGRE", &[]);
        assert!(
            !found.is_empty() && found.iter().all(|d| d.name.to_lowercase().contains("postgre"))
        );
    }
}
