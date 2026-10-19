# Rowcraft installers

The Tauri build produces native packages using the existing Rowcraft icon and application identifier, `dev.dglite.desktop`. Keep this identifier stable so later versions upgrade the same application.

| Platform | Architecture | Outputs | Installation |
| --- | --- | --- | --- |
| macOS 11+ | Apple Silicon, Intel (separate downloads) | `.dmg`, `.app` | Open the DMG and drag Rowcraft into Applications. |
| Linux | x86_64 | `.AppImage`, `.deb`, `.rpm` | Run the AppImage, or install the package for your distribution. |
| Windows | x64 | NSIS `-setup.exe` | Run the installer. It installs for the current user. |

Windows installers download WebView2 if it is missing, so that first installation requires internet access. Linux packages depend on system WebKitGTK 4.1; their package managers install the declared dependencies. The AppImage needs a compatible Linux system and may require FUSE 2. Builds use Ubuntu 22.04 as the Linux baseline; older distributions are not supported.

The app embeds its frontend and Rust database connectors. It does not require a frontend server, Rust, or pnpm on users' computers. JDBC connections require Java 11+ and vendor driver JARs. Starting the optional local 9router requires Node.js 22+ with npm and internet access on first start. External local router detection also uses `lsof` on macOS/Linux; DEB/RPM packages declare it as a dependency, while AppImage users must install it separately.

## Build on your computer

Install Rust stable, Node.js 24, pnpm 11.6.0, and the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/). Build each platform's installers on that operating system.

```sh
pnpm --dir apps/dg-desktop install --frozen-lockfile
pnpm --dir apps/dg-desktop installer
```

On macOS/Linux, `make installer` runs the same build. Output is under `target/release/bundle/` at the Cargo workspace root. A build with `--target <triple>` writes to `target/<triple>/release/bundle/` instead.

To build both Mac architectures from a Mac:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
pnpm --dir apps/dg-desktop installer --target aarch64-apple-darwin
pnpm --dir apps/dg-desktop installer --target x86_64-apple-darwin
```

Linux build machines also need the packaging tools installed by `.github/workflows/installers.yml`, including `rpm` and `patchelf`. Windows builds need Visual Studio C++ Build Tools and the Windows SDK. Tauri downloads the NSIS/WiX bundling tools as needed.

## Build all platforms in GitHub Actions

In **Actions → Desktop installers → Run workflow**, select the branch or tag to build and run it. Both manual runs and pushes of a matching version tag attach the macOS Apple Silicon, macOS Intel, Linux x64, and Windows x64 installers directly to the GitHub release for `v<VERSION>`, using the version in the app configuration. If the release does not exist, the workflow creates a draft; otherwise, it uploads to the existing release. A new release tag points to the commit selected for the run. Keep the app version aligned with the intended release before running manually.

The installers are also retained in the run's **Artifacts** section as a backup. Extract those workflow artifacts before using their installers; files in the release's **Assets** section can be downloaded directly.

For a versioned release:

1. Set the same version in `apps/dg-desktop/src-tauri/tauri.conf.json`, `apps/dg-desktop/package.json`, and `[workspace.package]` in `Cargo.toml`. Refresh and commit the lockfiles when Cargo or pnpm changes them.
2. Commit the source and push a matching tag, for example `v0.0.1` for the current version.
3. Wait for all four installer jobs. Review the installers in the GitHub release's **Assets** section, then publish the release when ready if it is still a draft. You can also run the workflow manually to create or add installers to the version's release.

The workflow needs permission to create releases and upload assets with its `GITHUB_TOKEN`; the installer job requests `contents: write`. If organization policy restricts that token, allow repository content write access for this workflow.

## Signing

macOS builds use ad-hoc signing by default. These are suitable for local testing but are not Developer ID signed or notarized; downloaded copies can be blocked by Gatekeeper. For distribution, add these repository Actions secrets:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64-encoded Developer ID Application `.p12` certificate |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the certificate |
| `APPLE_SIGNING_IDENTITY` | Full Developer ID Application identity |
| `APPLE_ID` | Apple account used for notarization |
| `APPLE_PASSWORD` | App-specific password for that account |
| `APPLE_TEAM_ID` | Apple Developer team ID |

The workflow exports only configured signing credentials to Tauri. When `APPLE_CERTIFICATE` is absent or empty, the certificate and its password stay unset so Tauri uses ad-hoc signing instead of trying to import an empty certificate. Empty notarization secrets also stay unset. A configured certificate must be a base64-encoded `.p12` containing both the certificate and private key, with the matching export password. Locally, install the certificate in Keychain and provide the same signing/notarization environment variables; `APPLE_SIGNING_IDENTITY` overrides the ad-hoc configuration. See [Tauri macOS signing](https://v2.tauri.app/distribute/sign/macos/).

Windows packages are unsigned by default. Before public distribution, configure a Windows signing certificate or signing service through Tauri's `bundle.windows.certificateThumbprint` / `signCommand` options and make its credentials available on the Windows runner. This setup does not include a certificate or a signing service. See [Tauri Windows signing](https://v2.tauri.app/distribute/sign/windows/).

The installers do not configure automatic application updates. Release artifacts are for manual installation and upgrades.

## Website and package managers

See [package distribution](package-distribution.md) to connect published release assets to the website and set up Homebrew, WinGet, Chocolatey, and Linux terminal installation.
