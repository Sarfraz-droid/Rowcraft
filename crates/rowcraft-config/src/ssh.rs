//! Shared SSH configurations. The SSH/SSL tab picks a
//! reusable "SSH configuration" owned by the IDE, which several data sources can share. The same
//! here: configurations live in `ssh.toml` next to `datasources.toml`, and a data source names one.
//!
//! Fields cover SSH hosts, authentication, host verification, and forwarding.
//! Secrets are stored in the OS credential store; legacy environment references remain supported.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result, config_dir};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SshAuth {
    #[default]
    Password,
    /// A private key file, with an optional passphrase.
    KeyPair,
    /// The running SSH agent (`SSH_AUTH_SOCK`).
    Agent,
}

/// What to do with a server whose key is not in the known-hosts file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostKeyPolicy {
    /// Refuse. The safe default: nothing is trusted that the user did not already know.
    #[default]
    Strict,
    /// Trust and remember a host seen for the first time; a *changed* key is still refused.
    AcceptNew,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshConfig {
    pub name: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub user: String,
    #[serde(default)]
    pub auth: SshAuth,
    /// Private key file for [`SshAuth::KeyPair`]; `~` is expanded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_file: Option<String>,
    /// Environment variable holding the password for [`SshAuth::Password`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_env: Option<String>,
    /// Environment variable holding the key passphrase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase_env: Option<String>,
    #[serde(default, skip_serializing_if = "crate::options::is_default")]
    pub password: crate::passwords::StoredSecret,
    #[serde(default, skip_serializing_if = "crate::options::is_default")]
    pub passphrase: crate::passwords::StoredSecret,
    #[serde(default)]
    pub host_key: HostKeyPolicy,
    /// Defaults to `~/.ssh/known_hosts`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_hosts_file: Option<String>,
}

impl SshConfig {
    pub fn port_or_default(&self) -> u16 {
        self.port.unwrap_or(22)
    }

    /// Checks the fields the chosen authentication needs.
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("SSH configuration: a name is required.".into());
        }
        if self.host.trim().is_empty() {
            return Err("SSH configuration: a host is required.".into());
        }
        if self.user.trim().is_empty() {
            return Err("SSH configuration: a user is required.".into());
        }
        match self.auth {
            SshAuth::KeyPair if self.key_file.as_deref().is_none_or(|k| k.trim().is_empty()) => {
                Err("SSH configuration: key pair authentication needs a private key file.".into())
            }
            _ => Ok(()),
        }
    }
}

/// Contents of `ssh.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshFile {
    #[serde(default, rename = "ssh")]
    pub configs: Vec<SshConfig>,
}

impl SshFile {
    pub fn get(&self, name: &str) -> Result<&SshConfig> {
        self.configs
            .iter()
            .find(|c| c.name == name)
            .ok_or_else(|| Error::SshNotFound(name.to_owned()))
    }

    pub fn add(&mut self, config: SshConfig) -> Result<()> {
        if self.configs.iter().any(|c| c.name == config.name) {
            return Err(Error::SshDuplicate(config.name));
        }
        self.configs.push(config);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<SshConfig> {
        let ix = self
            .configs
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| Error::SshNotFound(name.to_owned()))?;
        Ok(self.configs.remove(ix))
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
}

pub fn ssh_path() -> PathBuf {
    config_dir().join("ssh.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str) -> SshConfig {
        SshConfig {
            name: name.into(),
            host: "bastion.example.com".into(),
            user: "me".into(),
            ..Default::default()
        }
    }

    #[test]
    fn round_trips_and_omits_defaults() {
        let mut file = SshFile::default();
        file.add(sample("jump")).unwrap();
        let mut full = sample("keyed");
        full.port = Some(2222);
        full.auth = SshAuth::KeyPair;
        full.key_file = Some("~/.ssh/id_ed25519".into());
        full.passphrase_env = Some("KEY_PW".into());
        full.host_key = HostKeyPolicy::AcceptNew;
        full.known_hosts_file = Some("/tmp/kh".into());
        file.add(full).unwrap();
        let text = toml::to_string_pretty(&file).unwrap();
        assert!(text.contains("[[ssh]]"));
        assert_eq!(toml::from_str::<SshFile>(&text).unwrap(), file);
        let first = text.split("[[ssh]]").nth(1).unwrap();
        assert!(!first.contains("key_file") && !first.contains("known_hosts"), "{first}");
    }

    #[test]
    fn lookups_duplicates_and_removal() {
        let mut file = SshFile::default();
        file.add(sample("a")).unwrap();
        assert!(matches!(file.add(sample("a")), Err(Error::SshDuplicate(_))));
        assert!(file.get("a").is_ok());
        assert!(matches!(file.get("b"), Err(Error::SshNotFound(_))));
        assert!(file.remove("a").is_ok() && file.remove("a").is_err());
    }

    #[test]
    fn validation_names_what_is_missing() {
        assert!(sample("a").validate().is_ok());
        assert!(
            SshConfig { name: " ".into(), ..sample("a") }.validate().unwrap_err().contains("name")
        );
        assert!(
            SshConfig { host: "".into(), ..sample("a") }.validate().unwrap_err().contains("host")
        );
        assert!(
            SshConfig { user: "".into(), ..sample("a") }.validate().unwrap_err().contains("user")
        );
        let keyed = SshConfig { auth: SshAuth::KeyPair, ..sample("a") };
        assert!(keyed.validate().unwrap_err().contains("key file"));
        assert!(SshConfig { key_file: Some("k".into()), ..keyed }.validate().is_ok());
        assert!(SshConfig { auth: SshAuth::Agent, ..sample("a") }.validate().is_ok());
    }

    #[test]
    fn the_default_policy_is_strict_and_the_port_defaults_to_22() {
        assert_eq!(HostKeyPolicy::default(), HostKeyPolicy::Strict);
        assert_eq!(sample("a").port_or_default(), 22);
        assert_eq!(SshConfig { port: Some(2200), ..sample("a") }.port_or_default(), 2200);
    }

    #[test]
    fn a_missing_file_is_empty() {
        let path = std::env::temp_dir().join("rowcraft-no-such-dir/ssh.toml");
        assert!(SshFile::load(&path).unwrap().configs.is_empty());
    }
}
