//! User-defined drivers, stored in `drivers.toml` next to `datasources.toml`.
//!
//! A custom driver adds a catalog entry (name, default port, host or file target) that connects
//! through a native dialect or a JDBC driver loaded in an isolated Java subprocess.

use std::path::{Path, PathBuf};

use rowcraft_drivers::{CustomDriver, Target};
use serde::{Deserialize, Serialize};

use crate::{Error, Result, config_dir};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DriverTarget {
    /// Host and port.
    #[default]
    Network,
    /// A file or directory path.
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomDriverConfig {
    pub id: String,
    pub name: String,
    /// Dialect id, e.g. `"pg"`; see `rowcraft dialects`.
    pub dialect: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default)]
    pub target: DriverTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jdbc_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jdbc_url_template: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jdbc_jars: Vec<PathBuf>,
}

impl From<&CustomDriverConfig> for CustomDriver {
    fn from(c: &CustomDriverConfig) -> Self {
        CustomDriver {
            id: c.id.clone(),
            name: c.name.clone(),
            dialect: c.dialect.clone(),
            port: c.port,
            target: match c.target {
                DriverTarget::Network => Target::Network,
                DriverTarget::File => Target::File,
            },
        }
    }
}

impl CustomDriverConfig {
    pub fn validate(&self) -> std::result::Result<(), String> {
        CustomDriver::from(self).validate()?;
        if self.dialect == "jdbc" {
            if self.jdbc_jars.is_empty() {
                return Err("a JDBC driver needs at least one driver JAR file".into());
            }
            if self.jdbc_url_template.as_deref().is_none_or(|url| !url.starts_with("jdbc:")) {
                return Err("a JDBC driver needs a URL template starting with jdbc:".into());
            }
        }
        if let Some(template) = self.jdbc_url_template.as_deref() {
            let sample = template
                .replace("{host}", "localhost")
                .replace("{port}", "1234")
                .replace("{database}", "app")
                .replace("{user}", "user");
            crate::parse_url(&sample).map_err(|e| format!("invalid JDBC URL template: {e}"))?;
        }
        Ok(())
    }
}

/// Contents of `drivers.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverFile {
    #[serde(default, rename = "driver")]
    pub drivers: Vec<CustomDriverConfig>,
}

impl DriverFile {
    pub fn get(&self, id: &str) -> Result<&CustomDriverConfig> {
        self.drivers.iter().find(|d| d.id == id).ok_or_else(|| Error::DriverNotFound(id.to_owned()))
    }

    /// Validates the entry and refuses an id that is already in this file or in the built-in catalog.
    pub fn add(&mut self, driver: CustomDriverConfig) -> Result<()> {
        driver.validate().map_err(|message| Error::InvalidSetting { field: "driver", message })?;
        let builtin = rowcraft_drivers::find(&driver.id).is_some_and(|d| !d.custom);
        if builtin || self.drivers.iter().any(|d| d.id == driver.id) {
            return Err(Error::DriverDuplicate(driver.id));
        }
        self.drivers.push(driver);
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> Result<CustomDriverConfig> {
        let ix = self
            .drivers
            .iter()
            .position(|d| d.id == id)
            .ok_or_else(|| Error::DriverNotFound(id.to_owned()))?;
        Ok(self.drivers.remove(ix))
    }

    /// A missing file is an empty list.
    pub fn load(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => return Err(Error::Io { path: path.to_owned(), source }),
        };
        toml::from_str(&text)
            .map_err(|e| Error::Parse { path: path.to_owned(), message: e.to_string() })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let io = |source| Error::Io { path: path.to_owned(), source };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| Error::Parse { path: path.to_owned(), message: e.to_string() })?;
        std::fs::write(path, text).map_err(io)
    }

    /// Makes these drivers part of the catalog (`rowcraft_drivers::all`). Returns one message per entry
    /// that was rejected; the valid ones are registered regardless.
    pub fn register(&self) -> Vec<String> {
        rowcraft_drivers::set_custom(self.drivers.iter().map(CustomDriver::from).collect())
    }
}

pub fn drivers_path() -> PathBuf {
    config_dir().join("drivers.toml")
}

/// Loads `drivers.toml` and registers its drivers in the catalog. Call once at startup, before
/// anything reads `rowcraft_drivers`. Returns warnings (an unreadable file, rejected entries) for the
/// caller to show; a broken file never prevents the program from starting.
pub fn register_custom_drivers() -> Vec<String> {
    match DriverFile::load(&drivers_path()) {
        Ok(file) => file.register(),
        Err(e) => vec![e.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> CustomDriverConfig {
        CustomDriverConfig {
            id: id.into(),
            name: "QuestDB".into(),
            dialect: "pg".into(),
            port: Some(8812),
            target: DriverTarget::Network,
            jdbc_class: None,
            jdbc_url_template: None,
            jdbc_jars: Vec::new(),
        }
    }

    #[test]
    fn round_trips_and_omits_defaults() {
        let mut file = DriverFile::default();
        file.add(sample("questdb")).unwrap();
        let mut local = sample("localfile");
        local.port = None;
        local.target = DriverTarget::File;
        local.dialect = "sqlite".into();
        file.add(local).unwrap();
        let text = toml::to_string_pretty(&file).unwrap();
        assert!(text.contains("[[driver]]"));
        assert_eq!(toml::from_str::<DriverFile>(&text).unwrap(), file);
        assert!(!text.split("[[driver]]").nth(2).unwrap().contains("port"), "{text}");
    }

    #[test]
    fn jdbc_driver_requires_files_and_refuses_embedded_passwords() {
        let mut driver = sample("jdbc-test");
        driver.dialect = "jdbc".into();
        assert!(driver.validate().is_err());
        driver.jdbc_jars.push("vendor.jar".into());
        driver.jdbc_url_template = Some("jdbc:vendor://{host}:{port}/{database}".into());
        assert!(driver.validate().is_ok());
        driver.jdbc_url_template =
            Some("jdbc:vendor://{host}:{port}/{database}?password=secret".into());
        assert!(driver.validate().is_err());
    }

    #[test]
    fn minimal_entries_default_to_a_network_target() {
        let file: DriverFile =
            toml::from_str("[[driver]]\nid = \"x\"\nname = \"X\"\ndialect = \"pg\"\n").unwrap();
        assert_eq!(file.drivers[0].target, DriverTarget::Network);
        assert_eq!(file.drivers[0].port, None);
    }

    #[test]
    fn add_refuses_duplicates_builtin_ids_and_invalid_entries() {
        let mut file = DriverFile::default();
        file.add(sample("questdb")).unwrap();
        assert!(matches!(file.add(sample("questdb")), Err(Error::DriverDuplicate(_))));
        assert!(matches!(file.add(sample("postgresql")), Err(Error::DriverDuplicate(_))));
        assert!(matches!(file.add(sample("Not Valid")), Err(Error::InvalidSetting { .. })));
        assert!(file.get("questdb").is_ok());
        assert!(matches!(file.get("nope"), Err(Error::DriverNotFound(_))));
        assert!(file.remove("questdb").is_ok() && file.remove("questdb").is_err());
    }

    #[test]
    fn a_missing_file_is_empty() {
        let path = std::env::temp_dir().join("rowcraft-no-such-dir/drivers.toml");
        assert!(DriverFile::load(&path).unwrap().drivers.is_empty());
    }

    #[test]
    fn registering_adds_the_driver_to_the_catalog() {
        let mut file = DriverFile::default();
        file.add(sample("config-test-driver")).unwrap();
        assert!(file.register().is_empty());
        let d = rowcraft_drivers::find("config-test-driver").unwrap();
        assert!(d.custom && d.dialect == Some("pg") && d.port == Some(8812));
    }
}
