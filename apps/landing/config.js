// Set verified public destinations before publishing. Missing downloads show "Coming soon".
window.ROWCRAFT_CONFIG = {
  repositoryUrl: 'https://github.com/Sarfraz-droid/Rowcraft',
  downloads: {
    macos: { arm64: '', x64: '' }, // DMG: Apple Silicon and Intel
    windows: { exe: '', msi: '' }, // x64 installers
    linux: { appimage: '', deb: '', rpm: '' }, // x64 packages
  },
  // Enable only after publishing and testing the package in its registry.
  // Empty IDs show labeled command previews, never an active package claim.
  // Homebrew: 'owner/tap/rowcraft'; WinGet: 'Publisher.Rowcraft'; Chocolatey: 'rowcraft'.
  packages: { homebrew: '', winget: '', chocolatey: '' },
};
