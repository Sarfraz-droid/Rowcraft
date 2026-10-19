// Set verified public destinations before publishing. Missing downloads show "Coming soon".
window.ROWCRAFT_CONFIG = {
  repositoryUrl: 'https://github.com/Sarfraz-droid/Rowcraft',
  downloads: {
    macos: {
      arm64: 'https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft_0.0.1_aarch64.dmg',
      x64: 'https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft_0.0.1_x64.dmg',
    },
    windows: {
      exe: 'https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft_0.0.1_x64-setup.exe',
      msi: '', // Not published in v0.0.1.
    },
    linux: {
      appimage: 'https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft_0.0.1_amd64.AppImage',
      deb: 'https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft_0.0.1_amd64.deb',
      rpm: 'https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft-0.0.1-1.x86_64.rpm',
    },
  },
  // Enable only after publishing and testing the package in its registry.
  // Empty IDs show labeled command previews, never an active package claim.
  // Homebrew: 'owner/tap/rowcraft'; WinGet: 'Publisher.Rowcraft'; Chocolatey: 'rowcraft'.
  packages: { homebrew: '', winget: '', chocolatey: '' },
};
