/* Published links come from config.js; unpublished commands are labeled previews. */
(() => {
  const config = window.ROWCRAFT_CONFIG || {};
  const platforms = {
    macos: {
      name: 'macOS', note: 'macOS 11 or later. Choose your Mac’s processor.',
      files: [['arm64', 'Apple Silicon', 'DMG · M-series chips'], ['x64', 'Intel', 'DMG · Intel processors']],
      methods: [['homebrew', 'Homebrew']],
    },
    windows: {
      name: 'Windows', note: 'For x64 PCs. Choose your preferred installer.',
      files: [['exe', 'Windows installer', 'EXE · Current user'], ['msi', 'Windows MSI', 'MSI · Windows Installer']],
      methods: [['winget', 'WinGet'], ['chocolatey', 'Chocolatey']],
    },
    linux: {
      name: 'Linux', note: 'For x64 systems. Choose your distribution’s format.',
      files: [['appimage', 'AppImage', 'Portable · Requires compatible system libraries'], ['deb', 'Debian / Ubuntu', 'DEB package'], ['rpm', 'Fedora / RHEL', 'RPM package']],
      methods: [['deb', 'APT · Debian / Ubuntu'], ['rpm', 'DNF · Fedora / RHEL'], ['appimage', 'AppImage']],
    },
  };
  const query = selector => document.querySelector(selector);
  const detected = navigator.userAgentData?.platform || navigator.platform || '';
  let platform = /Mac/i.test(detected) ? 'macos' : /Win/i.test(detected) ? 'windows' : /Linux/i.test(detected) && !/Android/i.test(navigator.userAgent) ? 'linux' : 'macos';
  const method = query('#install-method');
  const code = query('#install-command');
  const copy = query('#copy-install');
  function https(value) {
    try { const url = new URL(value); return url.protocol === 'https:' ? url.href : null; } catch { return null; }
  }
  function renderCommand() {
    const key = method.value;
    let command = '';
    let note = '';
    let preview = false;
    query('#terminal-shell').textContent = platform === 'windows' ? 'PowerShell' : 'Terminal';
    if (platform === 'linux') {
      // Fixed filenames avoid shell interpolation of release asset names or URLs.
      const commands = {
        deb: 'sudo apt install ./rowcraft.deb',
        rpm: 'sudo dnf install ./rowcraft.rpm',
        appimage: 'chmod +x ./rowcraft.AppImage\n./rowcraft.AppImage',
      };
      const filename = { deb: 'rowcraft.deb', rpm: 'rowcraft.rpm', appimage: 'rowcraft.AppImage' }[key];
      command = commands[key];
      note = `Download the ${key === 'appimage' ? 'AppImage' : key.toUpperCase()} above, rename it to ${filename}, then run these commands from its folder.`;
      const linuxDownloads = config.downloads?.linux;
      preview = !https(typeof linuxDownloads === 'string' ? linuxDownloads : linuxDownloads?.[key]);
      if (preview) note = `After the package is published, download it and rename it to ${filename}. Run the commands from its folder.`;
      if (key === 'appimage') note += ' Install lsof separately; FUSE 2 may also be required.';
    } else {
      const id = config.packages?.[key];
      const valid = typeof id === 'string' && (key === 'homebrew' ? /^[a-zA-Z0-9_-]+\/[a-zA-Z0-9_-]+\/[a-zA-Z0-9_.-]+$/ : /^[a-zA-Z0-9][a-zA-Z0-9_.-]*$/).test(id);
      if (valid) {
        command = key === 'homebrew' ? `brew install --cask ${id}` : key === 'winget' ? `winget install --id ${id} --exact --source winget` : `choco install ${id}`;
        note = key === 'homebrew' ? 'Requires Homebrew. Run in Terminal.' : key === 'winget' ? 'Requires WinGet. Run in PowerShell or Windows Terminal.' : 'Requires Chocolatey. Run in an administrator PowerShell.';
      } else {
        const label = platforms[platform].methods.find(([value]) => value === key)[1];
        preview = true;
        command = key === 'homebrew' ? 'brew install --cask YOUR_OWNER/tap/rowcraft' : key === 'winget' ? 'winget install --id YOUR_PUBLISHER.Rowcraft --exact --source winget' : 'choco install rowcraft';
        note = `${label} support is not published yet. This is the command format for the future package.`;
        if (key === 'homebrew') note += ' Replace YOUR_OWNER with the published tap owner.';
        if (key === 'winget') note += ' Replace YOUR_PUBLISHER with the approved publisher ID.';
      }
    }
    code.textContent = preview ? `# Preview only. Requires a published package.\n${command}` : command;
    query('#install-note').textContent = note;
    query('#command-state').textContent = preview ? 'Command preview' : 'Ready to install';
    query('#command-state').classList.toggle('is-preview', preview);
    query('#install-feedback').textContent = '';
    copy.querySelector('span').textContent = preview ? 'Copy example' : 'Copy command';
  }
  function renderPlatform() {
    const info = platforms[platform];
    document.querySelectorAll('[data-platform]').forEach(button => button.setAttribute('aria-pressed', String(button.dataset.platform === platform)));
    query('#platform-title').textContent = `Download for ${info.name}`;
    query('#platform-note').textContent = info.note;
    query('#download-options').replaceChildren(...info.files.map(([key, label, description]) => {
      const configured = config.downloads?.[platform];
      // Older sites can still supply one general download destination per OS.
      const url = https(typeof configured === 'string' ? configured : configured?.[key]);
      const row = document.createElement('div'); row.className = 'download-option';
      const text = document.createElement('div');
      const title = document.createElement('strong'); title.textContent = label;
      const detail = document.createElement('span'); detail.textContent = description;
      text.append(title, detail);
      const action = document.createElement(url ? 'a' : 'button');
      action.className = url ? 'installer-link' : 'installer-unavailable';
      action.textContent = url ? 'Download' : 'Coming soon';
      if (url) { action.href = url; action.setAttribute('aria-label', `Download Rowcraft for ${info.name}: ${label}`); }
      else { action.type = 'button'; action.disabled = true; action.setAttribute('aria-label', `${label} download is not published yet`); }
      row.append(text, action); return row;
    }));
    method.replaceChildren(...info.methods.map(([value, label]) => {
      const option = document.createElement('option'); option.value = value; option.textContent = label; return option;
    }));
    renderCommand();
  }
  document.querySelectorAll('[data-platform]').forEach(button => button.addEventListener('click', () => { platform = button.dataset.platform; renderPlatform(); }));
  method.addEventListener('change', renderCommand);
  copy.addEventListener('click', async () => {
    const command = code.textContent;
    try {
      await navigator.clipboard.writeText(command);
      if (code.textContent === command) query('#install-feedback').textContent = 'Commands copied.';
    } catch { query('#install-feedback').textContent = 'Clipboard unavailable. Select the commands and copy them manually.'; }
  });
  renderPlatform();
  query('#installer').hidden = false;
})();
