//! Passwords entered in the desktop form live in the OS credential store, never TOML.

use serde::{Deserialize, Serialize};

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoredSecret {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(skip)]
    pub value: Option<String>,
}

impl std::fmt::Debug for StoredSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredSecret").field("value", &"<redacted>").finish_non_exhaustive()
    }
}

impl StoredSecret {
    pub fn entered(value: &str) -> Self {
        Self { account: None, value: (!value.is_empty()).then(|| value.to_owned()) }
    }

    pub fn resolve(&self) -> Result<Option<String>, String> {
        if let Some(value) = &self.value {
            return Ok(Some(value.clone()));
        }
        self.account.as_deref().map(load).transpose()
    }
}

pub fn new_account(label: &str) -> Result<String, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "The system clock is unavailable.".to_owned())?
        .as_nanos();
    Ok(format!("{}:{stamp}:{}:{label}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// Rolls back newly created credential entries if saving their configuration fails.
#[derive(Debug, Default)]
pub struct PendingSecrets(Vec<String>);

impl PendingSecrets {
    pub fn store(&mut self, label: &str, value: &str) -> Result<String, String> {
        let account = new_account(label)?;
        save(&account, value)?;
        self.0.push(account.clone());
        Ok(account)
    }

    pub fn store_secret(&mut self, label: &str, secret: &mut StoredSecret) -> Result<(), String> {
        if let Some(value) = &secret.value {
            secret.account = Some(self.store(label, value)?);
        }
        Ok(())
    }

    pub fn commit(mut self) {
        self.0.clear();
    }
}

impl Drop for PendingSecrets {
    fn drop(&mut self) {
        for account in &self.0 {
            let _ = delete(account);
        }
    }
}

#[cfg(target_os = "macos")]
const SERVICE: &str = "dev.rowcraft.database-passwords";

pub fn save(account: &str, password: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        security_framework::passwords::set_generic_password(SERVICE, account, password.as_bytes())
            .map_err(|error| format!("Could not save the password in Keychain: {error}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (account, password);
        Err("Saving passwords in the OS credential store is currently supported on macOS only."
            .into())
    }
}

pub fn load(account: &str) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let bytes = security_framework::passwords::get_generic_password(SERVICE, account)
            .map_err(|error| format!("Could not read the password from Keychain: {error}"))?;
        String::from_utf8(bytes).map_err(|_| "The saved password is not valid UTF-8.".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = account;
        Err("Reading passwords from Keychain requires macOS.".into())
    }
}

pub fn delete(account: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        security_framework::passwords::delete_generic_password(SERVICE, account)
            .map_err(|error| format!("Could not remove the password from Keychain: {error}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = account;
        Err("Removing passwords from Keychain requires macOS.".into())
    }
}
