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
  posthog: {
    projectToken: window.ROWCRAFT_POSTHOG_PROJECT_TOKEN,
    host: window.ROWCRAFT_POSTHOG_HOST,
  },
};

// Saved public release catalog; refreshed from GitHub when the page opens.
window.ROWCRAFT_CONFIG.releaseSnapshot = {
  "latestTag": "v0.0.1",
  "releases": [
    {
      "tag_name": "0.0.3",
      "html_url": "https://github.com/Sarfraz-droid/Rowcraft/releases/tag/0.0.3",
      "prerelease": false,
      "draft": false,
      "assets": []
    },
    {
      "tag_name": "v0.0.1",
      "html_url": "https://github.com/Sarfraz-droid/Rowcraft/releases/tag/v0.0.1",
      "prerelease": false,
      "draft": false,
      "assets": [
        {
          "name": "Rowcraft-0.0.1-1.x86_64.rpm",
          "browser_download_url": "https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft-0.0.1-1.x86_64.rpm"
        },
        {
          "name": "Rowcraft_0.0.1_aarch64.app.tar.gz",
          "browser_download_url": "https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft_0.0.1_aarch64.app.tar.gz"
        },
        {
          "name": "Rowcraft_0.0.1_aarch64.dmg",
          "browser_download_url": "https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft_0.0.1_aarch64.dmg"
        },
        {
          "name": "Rowcraft_0.0.1_amd64.AppImage",
          "browser_download_url": "https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft_0.0.1_amd64.AppImage"
        },
        {
          "name": "Rowcraft_0.0.1_amd64.deb",
          "browser_download_url": "https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft_0.0.1_amd64.deb"
        },
        {
          "name": "Rowcraft_0.0.1_x64-setup.exe",
          "browser_download_url": "https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft_0.0.1_x64-setup.exe"
        },
        {
          "name": "Rowcraft_0.0.1_x64.app.tar.gz",
          "browser_download_url": "https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft_0.0.1_x64.app.tar.gz"
        },
        {
          "name": "Rowcraft_0.0.1_x64.dmg",
          "browser_download_url": "https://github.com/Sarfraz-droid/Rowcraft/releases/download/v0.0.1/Rowcraft_0.0.1_x64.dmg"
        }
      ]
    }
  ]
};
