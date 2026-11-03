use std::{env, fs, path::PathBuf};

fn main() {
    stub_cli_sidecar();
    tauri_build::build();
}

/// `bundle.externalBin` makes tauri-build require the bundled CLI at `binaries/rowcraft-<triple>`,
/// which `scripts/prepare-cli-sidecar.mjs` produces for installer builds. Debug builds (clippy,
/// tests, `tauri dev`) don't need a real one, so a placeholder keeps them compiling. Release
/// builds get no placeholder: a missing CLI must fail the build, not ship a broken installer.
fn stub_cli_sidecar() {
    println!("cargo:rerun-if-changed=binaries");
    if env::var("PROFILE").is_ok_and(|p| p == "release") {
        return;
    }
    let (Ok(dir), Ok(target)) = (env::var("CARGO_MANIFEST_DIR"), env::var("TARGET")) else {
        return;
    };
    let exe = if target.contains("windows") { ".exe" } else { "" };
    let path = PathBuf::from(dir).join("binaries").join(format!("rowcraft-{target}{exe}"));
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, "placeholder: build the CLI with scripts/prepare-cli-sidecar.mjs\n");
}
