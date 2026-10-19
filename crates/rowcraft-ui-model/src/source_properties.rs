//! Displayable connection metadata. Never resolve credentials for this surface.

use rowcraft_config::DataSourceConfig;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SourceProperties {
    pub name: String,
    pub dialect: String,
    pub driver: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub user: Option<String>,
    pub connection_string: String,
    pub connection_type: Option<String>,
    pub comment: Option<String>,
    pub read_only: bool,
    pub ssl_mode: rowcraft_config::SslMode,
    pub auth: rowcraft_config::AuthType,
    pub ssh_tunnel: Option<String>,
    pub schema_pattern: Option<String>,
    pub object_filter: Option<String>,
}

fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

impl SourceProperties {
    pub fn from_config(source: &DataSourceConfig) -> Result<Self, String> {
        let config =
            source.effective().map_err(|_| "The connection URL could not be read.".to_owned())?;
        let driver = config.driver.as_deref().and_then(rowcraft_drivers::find);
        let port = config.port.or_else(|| driver.and_then(|driver| driver.port));
        let connection_string = if let Some(url) =
            config.url.as_deref().filter(|url| !url.trim().is_empty())
        {
            // effective() validates userinfo; omit arbitrary query/driver parameters, which can contain secrets.
            url.trim().split(['?', ';', '#']).next().unwrap_or_default().to_owned()
        } else if matches!(config.dialect.as_str(), "neo4j" | "graphql") {
            let host = config.host.as_deref().unwrap_or("localhost");
            let scheme = if config.ssl_mode == rowcraft_config::SslMode::Disable
                || (config.ssl_mode == rowcraft_config::SslMode::Auto
                    && matches!(host, "localhost" | "127.0.0.1" | "::1"))
            {
                "http"
            } else {
                "https"
            };
            let host = if host.contains(':') && !host.starts_with('[') {
                format!("[{host}]")
            } else {
                host.to_owned()
            };
            let mut url = reqwest::Url::parse(&format!(
                "{scheme}://{host}:{}",
                port.unwrap_or(if config.dialect == "neo4j" { 7474 } else { 4000 })
            ))
            .map_err(|_| "Invalid HTTP connection host.".to_owned())?;
            if config.dialect == "graphql" {
                url.set_path("/graphql");
            } else {
                url.path_segments_mut()
                    .map_err(|_| "Invalid endpoint.".to_owned())?
                    .clear()
                    .extend(["db", config.database.as_deref().unwrap_or("neo4j"), "query", "v2"]);
            }
            url.to_string()
        } else if config.dialect == "sqlite" {
            format!("sqlite://{}", config.database.as_deref().unwrap_or_default())
        } else {
            let scheme = match config.dialect.as_str() {
                "pg" | "pgbase" => "postgresql",
                "mssql" => "sqlserver",
                "mongo" => "mongodb",
                other => other,
            };
            let host = config.host.as_deref().unwrap_or("localhost");
            let host = if host.contains(':') && !host.starts_with('[') {
                format!("[{host}]")
            } else {
                host.to_owned()
            };
            let user = config
                .user
                .as_deref()
                .filter(|user| !user.is_empty())
                .map(|user| format!("{}@", encoded(user)))
                .unwrap_or_default();
            let port = port.map(|port| format!(":{port}")).unwrap_or_default();
            let database = config.database.as_deref().map(encoded).unwrap_or_default();
            format!("{scheme}://{user}{host}{port}/{database}")
        };
        Ok(Self {
            name: config.name,
            dialect: config.dialect,
            driver: config.driver,
            host: config.host,
            port,
            database: config.database,
            user: config.user,
            connection_string,
            connection_type: config.connection_type,
            comment: config.comment,
            read_only: config.read_only,
            ssl_mode: config.ssl_mode,
            auth: config.auth,
            ssh_tunnel: config.ssh_tunnel,
            schema_pattern: config.schema_pattern,
            object_filter: config.object_filter,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_apply_url_overrides_and_exclude_credentials_and_parameters() {
        let config = DataSourceConfig {
            name: "Production".into(),
            dialect: "mysql".into(),
            host: Some("old".into()),
            url: Some("mysql://reader@db.example:3307/app?token=private&ssl=true".into()),
            password: Some("private-password".into()),
            password_env: Some("PRIVATE_ENV".into()),
            password_keychain: Some("private-account".into()),
            ..Default::default()
        };
        let properties = SourceProperties::from_config(&config).unwrap();
        assert_eq!(properties.host.as_deref(), Some("db.example"));
        assert_eq!(properties.port, Some(3307));
        assert_eq!(properties.connection_string, "mysql://reader@db.example:3307/app");
        let json = serde_json::to_string(&properties).unwrap();
        for secret in ["private", "PRIVATE_ENV", "password_keychain"] {
            assert!(!json.contains(secret));
        }
    }

    #[test]
    fn connection_strings_support_ipv6_and_file_databases() {
        let config = DataSourceConfig {
            dialect: "pg".into(),
            host: Some("::1".into()),
            port: Some(5432),
            user: Some("a@b".into()),
            database: Some("my db".into()),
            ..Default::default()
        };
        assert_eq!(
            SourceProperties::from_config(&config).unwrap().connection_string,
            "postgresql://a%40b@[::1]:5432/my%20db"
        );
        let config = DataSourceConfig {
            dialect: "sqlite".into(),
            database: Some("/tmp/test.sqlite".into()),
            ..Default::default()
        };
        assert_eq!(
            SourceProperties::from_config(&config).unwrap().connection_string,
            "sqlite:///tmp/test.sqlite"
        );
    }
}
