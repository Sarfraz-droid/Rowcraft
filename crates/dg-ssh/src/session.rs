//! Connecting to the SSH host: host key check, then authentication with retries.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dg_config::{HostKeyPolicy, SshAuth, SshConfig};
use russh::client::{self, Handle};
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::AgentClient;
use russh::keys::{self, HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate, known_hosts};

use crate::{Credentials, Error, MAX_ATTEMPTS, Result, SecretKind};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Why the server's key was not accepted; read back to give the user a precise message.
#[derive(Debug, Clone)]
enum HostKeyProblem {
    Unknown { fingerprint: String },
    Changed { line: usize },
    Certificate,
}

pub(crate) struct Client {
    host: String,
    port: u16,
    policy: HostKeyPolicy,
    known_hosts: PathBuf,
    problem: Arc<Mutex<Option<HostKeyProblem>>>,
}

impl Client {
    fn reject(&self, problem: HostKeyProblem) -> std::result::Result<bool, russh::Error> {
        if let Ok(mut slot) = self.problem.lock() {
            *slot = Some(problem);
        }
        Ok(false)
    }
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = server_key else {
            return self.reject(HostKeyProblem::Certificate);
        };
        match known_hosts::check_known_hosts_path(&self.host, self.port, key, &self.known_hosts) {
            Ok(true) => Ok(true),
            Err(keys::Error::KeyChanged { line }) => self.reject(HostKeyProblem::Changed { line }),
            // Not recorded (or the file does not exist, or could not be read).
            Ok(false) | Err(_) => match self.policy {
                HostKeyPolicy::AcceptNew => {
                    // A key we cannot remember is still trusted for this one connection only if
                    // writing succeeds; failing to record it would silently turn this into
                    // "accept every time", so refuse instead.
                    match known_hosts::learn_known_hosts_path(
                        &self.host,
                        self.port,
                        key,
                        &self.known_hosts,
                    ) {
                        Ok(()) => Ok(true),
                        Err(_) => self.reject(HostKeyProblem::Unknown {
                            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
                        }),
                    }
                }
                HostKeyPolicy::Strict => self.reject(HostKeyProblem::Unknown {
                    fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
                }),
            },
        }
    }
}

/// `~` and `~/…` to the home directory.
pub(crate) fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if path == "~" => home(),
        None => PathBuf::from(path),
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn known_hosts_path(config: &SshConfig) -> PathBuf {
    match &config.known_hosts_file {
        Some(p) => expand_home(p),
        None => home().join(".ssh").join("known_hosts"),
    }
}

/// Connects, checks the host key and authenticates.
pub(crate) async fn connect(
    config: &SshConfig,
    credentials: &dyn Credentials,
) -> Result<Handle<Client>> {
    let (host, port) = (config.host.trim().to_owned(), config.port_or_default());
    let problem = Arc::new(Mutex::new(None));
    let handler = Client {
        host: host.clone(),
        port,
        policy: config.host_key,
        known_hosts: known_hosts_path(config),
        problem: problem.clone(),
    };
    let settings = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        ..client::Config::default()
    });
    let connecting = client::connect(settings, (host.as_str(), port), handler);
    let mut handle = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
        Err(_) => {
            return Err(Error::Connect { host, port, reason: "timed out".into() });
        }
        Ok(Err(e)) => {
            let known_hosts = known_hosts_path(config).display().to_string();
            let seen = problem.lock().ok().and_then(|p| p.clone());
            return Err(match seen {
                Some(HostKeyProblem::Unknown { fingerprint }) => {
                    Error::HostKeyUnknown { host, port, fingerprint, known_hosts }
                }
                Some(HostKeyProblem::Changed { line }) => {
                    Error::HostKeyChanged { host, port, line, known_hosts }
                }
                Some(HostKeyProblem::Certificate) => Error::HostKeyCertificate { host, port },
                None => Error::Connect { host, port, reason: e.to_string() },
            });
        }
        Ok(Ok(handle)) => handle,
    };
    authenticate(&mut handle, config, credentials).await?;
    Ok(handle)
}

fn failed(e: impl std::fmt::Display) -> Error {
    Error::AuthFailed(e.to_string())
}

async fn authenticate(
    handle: &mut Handle<Client>,
    config: &SshConfig,
    credentials: &dyn Credentials,
) -> Result<()> {
    let user = config.user.trim().to_owned();
    match config.auth {
        SshAuth::Password => {
            let mut last: Option<String> = None;
            for attempt in 0..MAX_ATTEMPTS {
                let Some(password) = credentials
                    .secret(config, SecretKind::Password, attempt, last.as_deref())
                    .await
                else {
                    return Err(match last {
                        Some(reason) => Error::AuthFailed(reason),
                        None => Error::NoCredentials(match &config.password_env {
                            Some(var) => format!(
                                "password not available: set the environment variable {var}"
                            ),
                            None => {
                                "no password available: enter an SSH password and make sure Keychain is unlocked"
                                    .into()
                            }
                        }),
                    });
                };
                let result =
                    handle.authenticate_password(user.clone(), password).await.map_err(failed)?;
                if result.success() {
                    return Ok(());
                }
                last = Some("the server rejected the password".into());
            }
            Err(Error::AuthFailed(format!("the password was rejected {MAX_ATTEMPTS} times")))
        }
        SshAuth::KeyPair => {
            let key = load_key(config, credentials).await?;
            let hash = handle.best_supported_rsa_hash().await.map_err(failed)?.flatten();
            let result = handle
                .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await
                .map_err(failed)?;
            if result.success() {
                Ok(())
            } else {
                Err(Error::AuthFailed("the server rejected the key".into()))
            }
        }
        SshAuth::Agent => {
            let mut agent = AgentClient::connect_env().await.map_err(|e| {
                Error::NoCredentials(format!("no SSH agent reachable (SSH_AUTH_SOCK): {e}"))
            })?;
            let identities = agent.request_identities().await.map_err(failed)?;
            if identities.is_empty() {
                return Err(Error::NoCredentials(
                    "the SSH agent holds no identities (ssh-add)".into(),
                ));
            }
            let hash = handle.best_supported_rsa_hash().await.map_err(failed)?.flatten();
            for identity in identities {
                let AgentIdentity::PublicKey { key, .. } = identity else { continue };
                let result = handle
                    .authenticate_publickey_with(user.clone(), key, hash, &mut agent)
                    .await
                    .map_err(failed)?;
                if result.success() {
                    return Ok(());
                }
            }
            Err(Error::AuthFailed("the server accepted none of the agent's keys".into()))
        }
    }
}

/// Loads the private key, asking for its passphrase when it is encrypted.
async fn load_key(config: &SshConfig, credentials: &dyn Credentials) -> Result<keys::PrivateKey> {
    let path = config.key_file.as_deref().unwrap_or_default();
    let file = expand_home(path);
    let key_error = |reason: String| Error::KeyFile { path: file.display().to_string(), reason };
    match keys::load_secret_key(&file, None) {
        Ok(key) => return Ok(key),
        Err(keys::Error::KeyIsEncrypted) => {}
        Err(e) => return Err(key_error(e.to_string())),
    }
    let mut last: Option<String> = None;
    for attempt in 0..MAX_ATTEMPTS {
        let Some(pass) =
            credentials.secret(config, SecretKind::KeyPassphrase, attempt, last.as_deref()).await
        else {
            return Err(match last {
                Some(reason) => key_error(reason),
                None => Error::NoCredentials(match &config.passphrase_env {
                    Some(var) => format!("the key is encrypted: set the environment variable {var} to its passphrase"),
                    None => "the key is encrypted: enter its key passphrase and make sure Keychain is unlocked".into(),
                }),
            });
        };
        match keys::load_secret_key(&file, Some(&pass)) {
            Ok(key) => return Ok(key),
            Err(e) => last = Some(format!("wrong passphrase ({e})")),
        }
    }
    Err(key_error(format!("the passphrase was rejected {MAX_ATTEMPTS} times")))
}
