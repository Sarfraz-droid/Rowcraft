//! App-owned, checksum-pinned JDBC downloads. No Maven, Python, or IDE installation needed.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const CENTRAL: &str = "https://repo.maven.apache.org/maven2/";
static INSTALLING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Deserialize)]
struct Catalog {
    drivers: Vec<Driver>,
}
#[derive(Debug, Deserialize)]
struct Driver {
    id: String,
    name: String,
    distribution: String,
    source: Option<String>,
    version: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
struct Package {
    version: String,
    files: Vec<Artifact>,
}
#[derive(Debug, Deserialize, Serialize)]
struct Artifact {
    path: String,
    sha256: String,
    size: u64,
}
#[derive(Debug, Serialize)]
pub struct PackageStatus {
    distribution: String,
    version: Option<String>,
    source: Option<String>,
    pub installed: bool,
    files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DownloadedFile {
    name: String,
    path: String,
    size: u64,
}

#[derive(Debug, Serialize)]
pub struct DownloadedDriver {
    id: String,
    name: String,
    version: String,
    available_version: Option<String>,
    verified: bool,
    directory: String,
    files: Vec<DownloadedFile>,
    size: u64,
}

#[derive(Debug, Serialize)]
pub struct DownloadOption {
    id: String,
    name: String,
    version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DownloadInventory {
    directory: String,
    packages: Vec<DownloadedDriver>,
    available: Vec<DownloadOption>,
    native: Vec<DownloadOption>,
}

#[derive(Debug, Deserialize)]
struct Receipt {
    driver: String,
    version: String,
}

fn receipt(directory: &Path, id: &str) -> Result<Receipt, String> {
    if fs::symlink_metadata(directory).map_err(error)?.file_type().is_symlink() {
        return Err("Linked driver directories cannot be managed here.".into());
    }
    let receipt: Receipt =
        serde_json::from_slice(&fs::read(directory.join("download-receipt.json")).map_err(error)?)
            .map_err(|e| format!("Could not read the download receipt for {id}: {e}"))?;
    if receipt.driver != id {
        return Err("The driver download receipt does not match this package.".into());
    }
    Ok(receipt)
}

pub fn inventory(config: &Path) -> Result<DownloadInventory, String> {
    let catalog: Catalog =
        serde_json::from_str(include_str!("../../../../drivers/catalog.json")).map_err(error)?;
    let directory = config.join("jdbc");
    let mut packages = Vec::new();
    let mut available = Vec::new();
    let mut native = Vec::new();
    for driver in catalog.drivers {
        if driver.distribution == "native" {
            native.push(DownloadOption { id: driver.id, name: driver.name, version: None });
            continue;
        }
        if driver.distribution != "maven" {
            continue;
        }
        available.push(DownloadOption {
            id: driver.id.clone(),
            name: driver.name.clone(),
            version: driver.version.clone(),
        });
        let path = directory.join(&driver.id);
        if !path.exists() {
            continue;
        }
        // Only packages carrying an app-owned receipt are managed; manual imports stay separate.
        if !path.join("download-receipt.json").exists() {
            continue;
        }
        let saved = receipt(&path, &driver.id)?;
        let mut files = Vec::new();
        for entry in fs::read_dir(&path).map_err(error)? {
            let entry = entry.map_err(error)?;
            if entry.file_type().map_err(error)?.is_file()
                && entry.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("jar"))
            {
                files.push(DownloadedFile {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: entry.path().to_string_lossy().into_owned(),
                    size: entry.metadata().map_err(error)?.len(),
                });
            }
        }
        files.sort_by(|a, b| a.name.cmp(&b.name));
        packages.push(DownloadedDriver {
            verified: status(&driver.id, config)?.installed,
            size: files.iter().map(|f| f.size).sum(),
            id: driver.id,
            name: driver.name,
            version: saved.version,
            available_version: driver.version,
            directory: path.to_string_lossy().into_owned(),
            files,
        });
    }
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    available.sort_by(|a, b| a.name.cmp(&b.name));
    native.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(DownloadInventory {
        directory: directory.to_string_lossy().into_owned(),
        packages,
        available,
        native,
    })
}

pub fn remove(id: &str, config: &Path) -> Result<(), String> {
    let (driver, _) = definitions(id)?;
    if driver.distribution != "maven" {
        return Err("Only downloaded JDBC packages can be removed here.".into());
    }
    let _guard = acquire_install_lock()?;
    let directory = config.join("jdbc").join(id);
    receipt(&directory, id)?;
    fs::remove_dir_all(&directory).map_err(|e| {
        format!("Could not remove {id}: {e}. Close connections using this driver and try again.")
    })
}

fn definitions(id: &str) -> Result<(Driver, Option<Package>), String> {
    let catalog: Catalog =
        serde_json::from_str(include_str!("../../../../drivers/catalog.json")).map_err(error)?;
    let driver =
        catalog.drivers.into_iter().find(|d| d.id == id).ok_or("Unknown built-in driver.")?;
    let mut packages: BTreeMap<String, Package> =
        serde_json::from_str(include_str!("../../../../drivers/downloads.lock.json"))
            .map_err(error)?;
    let package = packages.remove(id);
    if driver.distribution == "maven" {
        let package = package
            .as_ref()
            .ok_or("No download manifest for this driver. Upload vendor JARs instead.")?;
        if driver.version.as_deref() != Some(&package.version) || package.files.is_empty() {
            return Err("The driver download manifest needs updating.".into());
        }
        for file in &package.files {
            artifact_name(file)?;
        }
    }
    Ok((driver, package))
}

fn artifact_name(file: &Artifact) -> Result<&str, String> {
    if file.path.split('/').any(|s| {
        s.is_empty()
            || s == "."
            || s == ".."
            || !s.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    }) || !file.path.ends_with(".jar")
        || file.sha256.len() != 64
        || !file.sha256.chars().all(|c| c.is_ascii_hexdigit())
        || file.size == 0
    {
        return Err("Invalid pinned driver artifact.".into());
    }
    file.path.rsplit('/').next().ok_or("Missing artifact filename.".into())
}

fn verified(path: &Path, artifact: &Artifact) -> bool {
    use std::io::Read;
    let Ok(mut file) = fs::File::open(path) else { return false };
    if file.metadata().map(|m| m.len()).ok() != Some(artifact.size) {
        return false;
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => hash.update(&buffer[..n]),
            Err(_) => return false,
        }
    }
    format!("{:x}", hash.finalize()) == artifact.sha256
}

pub fn status(id: &str, config: &Path) -> Result<PackageStatus, String> {
    let (driver, package) = definitions(id)?;
    let directory = config.join("jdbc").join(id);
    let mut files = Vec::new();
    let installed = if let Some(package) = package {
        let expected: Vec<_> = package
            .files
            .iter()
            .map(|f| artifact_name(f).map(|name| directory.join(name)))
            .collect::<Result<_, _>>()?;
        let valid =
            package.files.iter().zip(&expected).all(|(artifact, path)| verified(path, artifact));
        if valid {
            files = expected.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        }
        valid
    } else {
        driver.distribution == "native"
    };
    Ok(PackageStatus {
        distribution: driver.distribution,
        version: driver.version,
        source: driver.source,
        installed,
        files,
    })
}

struct InstallGuard;
fn acquire_install_lock() -> Result<InstallGuard, String> {
    INSTALLING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "Another driver operation is running. Try again when it finishes.")?;
    Ok(InstallGuard)
}
impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::Release);
    }
}
struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        if !self.0.join("previous").exists() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

pub async fn install(
    id: &str,
    config: &Path,
    progress: impl Fn(String),
) -> Result<PackageStatus, String> {
    let (driver, package) = definitions(id)?;
    if driver.distribution != "maven" {
        return Err("This driver requires no automatic download. Use the vendor link for vendor-only drivers.".into());
    }
    let package = package.ok_or("No pinned driver package.")?;
    let _guard = acquire_install_lock()?;
    // Keep incomplete downloads outside jdbc/, where the bridge recursively discovers JARs.
    fs::create_dir_all(config).map_err(error)?;
    let staging_path = config.join(format!(".driver-download-{}", std::process::id()));
    fs::create_dir(&staging_path).map_err(|e| {
        format!(
            "Could not stage driver download: {e}. Check {} for an interrupted download.",
            staging_path.display()
        )
    })?;
    let staging = Staging(staging_path);
    let libraries = staging.0.join("libraries");
    fs::create_dir(&libraries).map_err(error)?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(error)?;
    for (index, artifact) in package.files.iter().enumerate() {
        let name = artifact_name(artifact)?;
        progress(format!("Downloading file {} of {}: {name}", index + 1, package.files.len()));
        let mut response = client
            .get(format!("{CENTRAL}{}", artifact.path))
            .send()
            .await
            .map_err(|e| {
                format!("Could not download {name}: {e}. Check your internet connection and retry.")
            })?
            .error_for_status()
            .map_err(error)?;
        let mut file = fs::File::create(libraries.join(name)).map_err(error)?;
        let mut size = 0u64;
        let mut hash = Sha256::new();
        while let Some(chunk) = response.chunk().await.map_err(error)? {
            size += chunk.len() as u64;
            if size > artifact.size {
                return Err(format!("Download size mismatch for {name}. Retry the download."));
            }
            hash.update(&chunk);
            file.write_all(&chunk).map_err(error)?;
        }
        if size != artifact.size || format!("{:x}", hash.finalize()) != artifact.sha256 {
            return Err(format!(
                "Checksum verification failed for {name}. Nothing was installed; retry the download."
            ));
        }
        file.sync_all().map_err(error)?;
    }
    let receipt = serde_json::json!({
        "driver": id, "version": package.version, "source": driver.source,
        "files": package.files.iter().map(|file| Ok((artifact_name(file)?.to_owned(), file.sha256.clone())))
            .collect::<Result<BTreeMap<_, _>, String>>()?,
    });
    fs::write(
        libraries.join("download-receipt.json"),
        serde_json::to_vec_pretty(&receipt).map_err(error)?,
    )
    .map_err(error)?;
    progress("Installing verified driver files…".into());
    publish(&libraries, &config.join("jdbc").join(id), &staging.0)?;
    status(id, config)
}

fn publish(libraries: &Path, target: &Path, staging: &Path) -> Result<(), String> {
    fs::create_dir_all(target.parent().ok_or("Missing driver directory.")?).map_err(error)?;
    let backup = staging.join("previous");
    if target.exists() {
        fs::rename(target, &backup).map_err(error)?;
    }
    if let Err(e) = fs::rename(libraries, target) {
        if backup.exists() {
            if let Err(restore) = fs::rename(&backup, target) {
                // Preserve the previous driver for recovery if the filesystem refuses restoration.
                let recovery = staging.with_extension("recovery");
                if fs::rename(staging, &recovery).is_err() {
                    return Err(format!(
                        "Install failed: {e}; restore failed: {restore}. Recover files from {}",
                        backup.display()
                    ));
                }
                return Err(format!(
                    "Install failed: {e}; restore failed: {restore}. Previous files are in {}",
                    recovery.display()
                ));
            }
        }
        return Err(error(e));
    }
    if backup.exists() {
        fs::remove_dir_all(&backup).map_err(error)?;
    }
    Ok(())
}

pub fn open_vendor(id: &str) -> Result<(), String> {
    let (driver, _) = definitions(id)?;
    if driver.distribution != "vendor" {
        return Err("This driver is available inside Rowcraft.".into());
    }
    let url = driver
        .source
        .filter(|url| url.starts_with("https://"))
        .ok_or("No vendor download link available.")?;
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(&url).status();
    #[cfg(target_os = "windows")]
    let status =
        std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", &url]).status();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let status = std::process::Command::new("xdg-open").arg(&url).status();
    if status.map_err(error)?.success() {
        Ok(())
    } else {
        Err(format!("Could not open the browser. Visit {url}"))
    }
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inventory_and_removal_only_manage_receipted_packages() {
        let root =
            std::env::temp_dir().join(format!("rowcraft-driver-inventory-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let _cleanup = Staging(root.clone());
        assert!(inventory(&root).unwrap().packages.is_empty());
        let package = root.join("jdbc/hsqldb_local");
        fs::create_dir_all(&package).unwrap();
        fs::write(
            package.join("download-receipt.json"),
            br#"{"driver":"hsqldb_local","version":"2.7.4"}"#,
        )
        .unwrap();
        fs::write(package.join("damaged.jar"), b"bad").unwrap();
        let manual = root.join("jdbc/duckdb");
        fs::create_dir(&manual).unwrap();
        fs::write(manual.join("manual.jar"), b"manual").unwrap();
        let list = inventory(&root).unwrap();
        assert_eq!(list.available.len(), 24);
        assert!(
            list.native
                .iter()
                .any(|driver| driver.id == "postgresql" && driver.name == "PostgreSQL")
        );
        assert!(!list.native.iter().any(|driver| driver.id == "hsqldb_local"));
        assert_eq!(list.packages.len(), 1);
        assert_eq!(list.packages[0].name, "HSQLDB (Local)");
        assert_eq!(list.packages[0].version, "2.7.4");
        assert_eq!(list.packages[0].size, 3);
        assert_eq!(list.packages[0].files.len(), 1);
        assert!(!list.packages[0].verified);
        assert!(remove("../duckdb", &root).is_err());
        assert!(remove("postgresql", &root).is_err());
        assert!(remove("duckdb", &root).is_err());
        fs::write(
            package.join("download-receipt.json"),
            br#"{"driver":"duckdb","version":"2.7.4"}"#,
        )
        .unwrap();
        assert!(remove("hsqldb_local", &root).is_err());
        assert!(package.join("damaged.jar").exists());
        fs::write(
            package.join("download-receipt.json"),
            br#"{"driver":"hsqldb_local","version":"2.7.4"}"#,
        )
        .unwrap();
        remove("hsqldb_local", &root).unwrap();
        assert!(!package.exists());
        assert!(manual.join("manual.jar").exists());
    }
    #[test]
    #[ignore = "downloads a real package from Maven Central into temporary storage"]
    fn live_download_installs_a_complete_verified_driver() {
        let root =
            std::env::temp_dir().join(format!("rowcraft-live-driver-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let cleanup = Staging(root.clone());
        let runtime = tauri::async_runtime::block_on(install("hsqldb_local", &root, |message| {
            println!("{message}")
        }))
        .unwrap();
        assert!(runtime.installed);
        assert!(!runtime.files.is_empty());
        assert!(status("hsqldb_local", &root).unwrap().installed);
        fs::write(&runtime.files[0], b"corrupt").unwrap();
        assert!(!status("hsqldb_local", &root).unwrap().installed);
        drop(cleanup);
        assert!(!root.exists());
    }
    #[test]
    fn every_maven_driver_has_a_complete_locked_classpath() {
        let catalog: Catalog =
            serde_json::from_str(include_str!("../../../../drivers/catalog.json")).unwrap();
        for driver in catalog.drivers {
            assert!(definitions(&driver.id).is_ok(), "{}", driver.id);
        }
        assert!(definitions("../../anything").is_err());
    }
    #[test]
    fn rejects_unsafe_artifact_paths() {
        for path in ["../driver.jar", "/driver.jar", "https://other/driver.jar", "a\\b.jar"] {
            assert!(
                artifact_name(&Artifact { path: path.into(), sha256: "a".repeat(64), size: 1 })
                    .is_err()
            );
        }
    }
    #[test]
    fn detects_corruption_and_replaces_packages_without_old_jars() {
        let root =
            std::env::temp_dir().join(format!("rowcraft-driver-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let old = root.join("jdbc/test");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("old.jar"), b"old").unwrap();
        let staging = root.join("staging");
        let libraries = staging.join("libraries");
        fs::create_dir_all(&libraries).unwrap();
        let path = libraries.join("new.jar");
        fs::write(&path, b"new").unwrap();
        let artifact = Artifact {
            path: "new.jar".into(),
            sha256: format!("{:x}", Sha256::digest(b"new")),
            size: 3,
        };
        assert!(verified(&path, &artifact));
        fs::write(&path, b"bad").unwrap();
        assert!(!verified(&path, &artifact));
        publish(&libraries, &old, &staging).unwrap();
        assert!(old.join("new.jar").exists());
        assert!(!old.join("old.jar").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
