# Publish Rowcraft downloads and terminal installation

The landing page is ready for installers and package-manager commands. No public release URLs, Homebrew tap, WinGet listing, or Chocolatey package are configured in this checkout. Publishing them is separate from adding the UI.

## 1. Publish installers

Follow [installer builds and signing](installers.md). Push the matching `v<VERSION>` tag, wait for all four jobs, review the draft release, then publish it. Use signed/notarized macOS installers for distribution and configure Windows signing. Download and test each architecture on its intended OS.

Copy the actual public HTTPS asset URLs into `apps/landing/config.js`: the two macOS DMGs, Windows EXE/MSI, and Linux AppImage/DEB/RPM. Use versioned URLs from the published release, with the exact filenames produced by Tauri. Workflow artifacts and draft releases are not public download links. Do not guess release asset names.

For the current version (`0.0.3`), once the source and workflow are committed to your public GitHub repository:

```sh
git tag v0.0.3
git push origin v0.0.3
```

Run these yourself when ready to trigger the release build. Publishing the resulting draft is a separate action in GitHub Releases. Set the version consistently before tagging a later release; see the installer guide for all version files.

## 2. macOS: Homebrew cask

Create a public `homebrew-tap` repository under your GitHub owner. Add `Casks/rowcraft.rb` with the release version, architecture-specific DMG URLs and SHA-256 checksums, name, description, homepage, and `app "Rowcraft.app"`. Use `on_arm` / `on_intel` blocks for the different URLs and checksums, and declare the macOS requirement. Compute each checksum from its final release asset with `shasum -a 256 <installer.dmg>`.

After the tap is published, test on both Mac architectures:

```sh
brew install --cask YOUR_OWNER/tap/rowcraft
```

Replace `YOUR_OWNER` with the actual owner. Only after successful installation, set `packages.homebrew` to that full cask token. Update the cask version, URLs, and checksums with each release. Start with your own tap; a homebrew/cask submission has additional acceptance requirements.

This is the cask file structure. Replace every uppercase placeholder with your real URL or checksum; these example values are not installable:

```ruby
cask "rowcraft" do
  version "0.0.3"

  on_arm do
    sha256 "APPLE_SILICON_DMG_SHA256"
    url "https://github.com/YOUR_OWNER/YOUR_REPO/releases/download/v#{version}/EXACT_ARM_DMG_FILENAME"
  end

  on_intel do
    sha256 "INTEL_DMG_SHA256"
    url "https://github.com/YOUR_OWNER/YOUR_REPO/releases/download/v#{version}/EXACT_INTEL_DMG_FILENAME"
  end

  name "Rowcraft"
  desc "Database workbench for SQL and data exploration"
  homepage "https://github.com/YOUR_OWNER/YOUR_REPO"
  depends_on macos: ">= :big_sur"
  app "Rowcraft.app"
end
```

Place this file in `Casks/rowcraft.rb` in `YOUR_OWNER/homebrew-tap`. Replace the URLs with the exact signed release asset names rather than inferring them from the architecture.

References: [creating a tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap), [cask format and architecture support](https://docs.brew.sh/Cask-Cookbook).

## 3. Windows: WinGet, optionally Chocolatey

WinGet is a suitable first Windows channel. On Windows, install Microsoft's manifest creator:

```powershell
winget install wingetcreate
wingetcreate new <PUBLIC_VERSIONED_INSTALLER_URL>
```

Use the actual EXE or MSI URL and complete publisher, identifier, version, license, architecture, scope, installer type, and other required metadata from the project and installer. The creator can prepare a PR to `microsoft/winget-pkgs`; review it before submission. Wait for acceptance and index availability, then test installation and upgrades. Set `packages.winget` to the accepted identifier. The site generates `winget install --id <ID> --exact --source winget`.

If you also want Chocolatey, create a package with `choco new rowcraft`. Populate its nuspec metadata and install/uninstall scripts using Chocolatey's documented helpers. Reference the versioned installer URL and SHA-256 checksum, and verify the silent arguments for the selected Tauri installer. Test in a clean Windows VM, publish to your chosen feed, and wait for community moderation if using the community feed. The landing page's command uses the default configured feed; set `packages.chocolatey` only when the package resolves there. It generates `choco install <PACKAGE>`.

References: [WinGet manifests and submission](https://learn.microsoft.com/en-us/windows/package-manager/package/manifest), [Chocolatey package creation](https://docs.chocolatey.org/en-us/create/create-packages/).

## 4. Linux: downloadable packages first

The simplest initial support is local DEB/RPM installation. The UI tells visitors to download and rename the corresponding file, then use:

```sh
# Debian / Ubuntu, from the downloaded file's folder
sudo apt install ./rowcraft.deb

# Fedora / RHEL, from the downloaded file's folder
sudo dnf install ./rowcraft.rpm

# AppImage, from the downloaded file's folder
chmod +x ./rowcraft.AppImage
./rowcraft.AppImage
```

APT and DNF resolve declared package dependencies. These commands do not create an upstream package repository. AppImage users need compatible system libraries and must install `lsof` separately; FUSE 2 may also be needed. Linux ARM builds are not currently included in CI.

For `sudo apt install rowcraft` or `sudo dnf install rowcraft` without a local file, host and maintain signed APT/RPM repositories: publish package indexes and signing keys, document repository setup, and test updates. A `.deb` or `.rpm` uploaded to GitHub alone does not register it in a distribution repository. Flatpak/Flathub is another distribution option, requiring its own manifest, runtime selection, and sandbox integration testing.

References: [Tauri Debian packaging](https://v2.tauri.app/distribute/debian/), [Tauri RPM packaging](https://v2.tauri.app/distribute/rpm/).

## Release maintenance

For each version, publish and test the installers first, then update package URLs/checksums and registry manifests, then update the landing page config. Keep package IDs empty until their channel works. A future release job can generate checksums and propose tap/manifest updates once the public repository and publisher identities are established.

These channels install the Rowcraft desktop app. To distribute the separate headless `dg` CLI, build its binaries and package a Homebrew formula or equivalent CLI package separately; the desktop cask does not automatically expose `dg` on PATH.

## Connect the published channels to the website

Edit `apps/landing/config.js` with the real repository URL, public asset URLs, and accepted registry IDs. The keys are:

| Channel | Configuration key | Value after publication |
| --- | --- | --- |
| Apple Silicon DMG | `downloads.macos.arm64` | Actual public ARM DMG URL |
| Intel DMG | `downloads.macos.x64` | Actual public Intel DMG URL |
| Windows EXE | `downloads.windows.exe` | Actual public installer URLs |
| Linux packages | `downloads.linux.appimage` / `deb` / `rpm` | Actual public package URLs |
| Homebrew | `packages.homebrew` | `YOUR_OWNER/tap/rowcraft` with your real owner |
| WinGet | `packages.winget` | Identifier accepted by the WinGet registry |
| Chocolatey | `packages.chocolatey` | Package name published on the default feed |

Preview with `make landing`. Unconfigured registry channels display example commands, explicitly marked as requiring publication; configured channels display the real command. Filling in config does not create or publish any registry package. Deploy the static `apps/landing` directory after the config is updated.
