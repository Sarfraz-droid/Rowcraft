//! SSH tunnels: local port forwarding through an SSH host, in pure Rust (`russh`).
//!
//! Shaped after DataGrip's `intellij.database.ssh` classes: a shared SSH configuration, a tunnel
//! establisher that asks for credentials again when authentication fails, an optional forced
//! local port (an error when it is occupied, and when several tunnels are needed), and one tunnel
//! per host:port found in the data source, so cluster databases get a tunnel per node.

mod forward;
mod session;
mod target;

use std::net::SocketAddr;

use async_trait::async_trait;
pub use dg_config::SshConfig;
pub use forward::Tunnels;
pub use target::{Target, parse_targets};

/// How many times a rejected password or a key passphrase is asked for again.
pub const MAX_ATTEMPTS: u32 = 3;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("SSH: {0}")]
    Config(String),
    #[error("SSH: cannot connect to {host}:{port}: {reason}")]
    Connect { host: String, port: u16, reason: String },
    #[error(
        "SSH: the key of {host}:{port} ({fingerprint}) is not in {known_hosts}; \
         add it (ssh-keyscan) or set the host key policy to accept-new"
    )]
    HostKeyUnknown { host: String, port: u16, fingerprint: String, known_hosts: String },
    #[error(
        "SSH: the key of {host}:{port} CHANGED (line {line} of {known_hosts}); refusing to connect"
    )]
    HostKeyChanged { host: String, port: u16, line: usize, known_hosts: String },
    #[error("SSH: {host}:{port} offered a certificate host key, which is not supported yet")]
    HostKeyCertificate { host: String, port: u16 },
    #[error("SSH: authentication failed: {0}")]
    AuthFailed(String),
    #[error("SSH: {0}")]
    NoCredentials(String),
    #[error("SSH: cannot use key file {path}: {reason}")]
    KeyFile { path: String, reason: String },
    #[error("SSH: forced local port {0} is already in use")]
    ForcedPortOccupied(u16),
    #[error(
        "SSH: unable to create multiple tunnels ({0}) when the configuration forces local port {1}"
    )]
    ForcedPortWithMultipleTunnels(usize, u16),
    #[error("SSH: {0}")]
    Target(String),
    #[error("SSH: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    Password,
    KeyPassphrase,
}

/// Where secrets come from. `attempt` counts from 0 and grows each time the previous answer was
/// rejected, so an interactive implementation (the GUI) can prompt again with `last_error`;
/// returning `None` stops asking.
#[async_trait]
pub trait Credentials: Send + Sync {
    async fn secret(
        &self,
        config: &SshConfig,
        kind: SecretKind,
        attempt: u32,
        last_error: Option<&str>,
    ) -> Option<String>;
}

/// Reads entered or Keychain credentials, then legacy environment references. Only the first
/// attempt is answered, since retrying the same secret would give the same wrong answer.
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvCredentials;

#[async_trait]
impl Credentials for EnvCredentials {
    async fn secret(
        &self,
        config: &SshConfig,
        kind: SecretKind,
        attempt: u32,
        _last_error: Option<&str>,
    ) -> Option<String> {
        if attempt > 0 {
            return None;
        }
        let saved = match kind {
            SecretKind::Password => &config.password,
            SecretKind::KeyPassphrase => &config.passphrase,
        };
        if saved.value.is_some() || saved.account.is_some() {
            return saved.resolve().ok().flatten();
        }
        let var = match kind {
            SecretKind::Password => config.password_env.as_deref(),
            SecretKind::KeyPassphrase => config.passphrase_env.as_deref(),
        }?;
        std::env::var(var).ok()
    }
}

/// Opens tunnels. A trait so the engine can be tested without an SSH server.
#[async_trait]
pub trait TunnelOpener: Send + Sync {
    async fn open(
        &self,
        config: &SshConfig,
        credentials: &dyn Credentials,
        targets: &[Target],
        forced_local_port: Option<u16>,
    ) -> Result<Tunnels>;
}

/// The real thing.
#[derive(Debug, Default, Clone, Copy)]
pub struct RusshOpener;

#[async_trait]
impl TunnelOpener for RusshOpener {
    async fn open(
        &self,
        config: &SshConfig,
        credentials: &dyn Credentials,
        targets: &[Target],
        forced_local_port: Option<u16>,
    ) -> Result<Tunnels> {
        forward::open(config, credentials, targets, forced_local_port).await
    }
}

impl Tunnels {
    /// A set that just reports `endpoints`, for tests of code that consumes tunnels. `guard` is
    /// dropped with the set, which lets a test see when the tunnels are released.
    pub fn from_parts(
        endpoints: Vec<SocketAddr>,
        guard: Box<dyn std::any::Any + Send + Sync>,
    ) -> Self {
        forward::from_parts(endpoints, guard)
    }
}

#[cfg(test)]
mod credential_tests {
    use super::*;

    #[tokio::test]
    async fn entered_passwords_and_key_passphrases_reach_authentication_without_environment_variables()
     {
        let config = SshConfig {
            password: dg_config::passwords::StoredSecret::entered(" demo password "),
            passphrase: dg_config::passwords::StoredSecret::entered(" demo passphrase "),
            ..Default::default()
        };
        assert_eq!(
            EnvCredentials.secret(&config, SecretKind::Password, 0, None).await.as_deref(),
            Some(" demo password ")
        );
        assert_eq!(
            EnvCredentials.secret(&config, SecretKind::KeyPassphrase, 0, None).await.as_deref(),
            Some(" demo passphrase ")
        );
        assert!(EnvCredentials.secret(&config, SecretKind::Password, 1, None).await.is_none());
    }
}
