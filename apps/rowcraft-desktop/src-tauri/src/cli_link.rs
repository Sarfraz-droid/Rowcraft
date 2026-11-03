//! Puts the bundled `rowcraft` command line on the user's PATH.
//!
//! The installers ship the CLI next to the app binary (`bundle.externalBin`). Most already put it
//! on PATH: DEB/RPM install it to `/usr/bin`, and the Windows installer adds its folder to the
//! user PATH. Two layouts have no installer step, so the app links the CLI itself on launch:
//!
//! * macOS: the DMG is drag-and-drop. The CLI lives inside `Rowcraft.app`, so it is symlinked.
//! * Linux AppImage: the bundle mounts at a new path on every run, so the CLI is copied out.
//!
//! The link goes to `/usr/local/bin` when that is writable (no prompt, already on PATH), else
//! `~/.local/bin`. A file we did not create is never overwritten; ownership is recorded in a
//! marker under the config directory.

use std::{
    fs,
    path::{Path, PathBuf},
};

const CLI_NAME: &str = "rowcraft";
const MARKER: &str = "cli-link";

/// Where the link ended up and whether a new shell will find it without editing any profile.
#[derive(Debug, serde::Serialize)]
pub struct CliLink {
    pub path: String,
    pub on_path: bool,
}

/// Link `source` as `dir/rowcraft` (copy it when `copy`), refreshing a link we made before.
/// Refuses to replace anything else. `marker` records the path we manage.
#[cfg(unix)]
fn link_into(source: &Path, dir: &Path, marker: &Path, copy: bool) -> Result<PathBuf, String> {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let target = dir.join(CLI_NAME);
    let managed = fs::read_to_string(marker).is_ok_and(|m| Path::new(m.trim()) == target);
    if fs::symlink_metadata(&target).is_ok() {
        if !managed {
            return Err(format!(
                "{} already exists and was not installed by Rowcraft.",
                target.display()
            ));
        }
        fs::remove_file(&target).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    if copy {
        // Copy beside the target, then rename, so a running CLI is never half-written.
        let staging = dir.join(format!(".{CLI_NAME}.tmp"));
        fs::copy(source, &staging).map_err(|e| e.to_string())?;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        fs::rename(&staging, &target).map_err(|e| e.to_string())?;
    } else {
        symlink(source, &target).map_err(|e| e.to_string())?;
    }
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(marker, target.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    Ok(target)
}

fn on_path(dir: &Path) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d == dir))
}

/// The CLI shipped inside this installation, if it is a real build (not the debug placeholder).
fn bundled_cli() -> Option<PathBuf> {
    let path = std::env::current_exe().ok()?.parent()?.join(CLI_NAME);
    (!cfg!(debug_assertions) && path.is_file()).then_some(path)
}

#[cfg(unix)]
pub fn install() -> Result<CliLink, String> {
    let source = bundled_cli().ok_or("This build does not include the command line.")?;
    let marker = rowcraft_config::config_dir().join(MARKER);
    let copy = std::env::var_os("APPIMAGE").is_some();

    let mut dirs = Vec::new();
    if cfg!(target_os = "macos") && !copy {
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/bin"));
    }
    let mut last = "Could not find a folder to install the command line into.".to_string();
    for dir in dirs {
        match link_into(&source, &dir, &marker, copy) {
            Ok(path) => {
                return Ok(CliLink { on_path: on_path(&dir), path: path.to_string_lossy().into() });
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[cfg(not(unix))]
pub fn install() -> Result<CliLink, String> {
    Err("The Windows installer puts the command line on PATH.".into())
}

/// Best-effort on launch, only where the installer cannot do it. Failures are not user-facing:
/// the `install_cli` command reports them when asked.
pub fn install_on_launch() {
    if cfg!(target_os = "macos") || std::env::var_os("APPIMAGE").is_some() {
        std::thread::spawn(|| {
            let _ = install();
        });
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rowcraft-cli-link-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn links_then_refreshes_its_own_link() {
        let root = scratch("refresh");
        let (source, marker, bin) =
            (root.join("src-cli"), root.join("cfg/cli-link"), root.join("bin"));
        fs::write(&source, "v1").unwrap();
        let target = link_into(&source, &bin, &marker, false).unwrap();
        assert_eq!(fs::read_link(&target).unwrap(), source);
        // A later launch (new app version) re-links over the one we made.
        let moved = root.join("src-cli-2");
        fs::write(&moved, "v2").unwrap();
        link_into(&moved, &bin, &marker, false).unwrap();
        assert_eq!(fs::read_link(&target).unwrap(), moved);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copies_and_marks_executable() {
        use std::os::unix::fs::PermissionsExt;
        let root = scratch("copy");
        let (source, marker, bin) = (root.join("src-cli"), root.join("cli-link"), root.join("bin"));
        fs::write(&source, "binary").unwrap();
        let target = link_into(&source, &bin, &marker, true).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "binary");
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o111, 0o111);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn never_overwrites_a_file_it_did_not_create() {
        let root = scratch("foreign");
        let (source, marker, bin) = (root.join("src-cli"), root.join("cli-link"), root.join("bin"));
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join(CLI_NAME), "someone else's rowcraft").unwrap();
        fs::write(&source, "ours").unwrap();
        assert!(link_into(&source, &bin, &marker, false).is_err());
        assert_eq!(fs::read_to_string(bin.join(CLI_NAME)).unwrap(), "someone else's rowcraft");
        fs::remove_dir_all(root).unwrap();
    }
}
