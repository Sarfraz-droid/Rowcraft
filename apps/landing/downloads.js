/* Published links come from config.js; unpublished commands are labeled previews. */
(() => {
  const config = window.ROWCRAFT_CONFIG || {};
  const platforms = {
    macos: {
      name: 'macOS', note: 'macOS 11 or later. Choose your Mac’s processor.',
      files: [['arm64', 'Apple Silicon', 'DMG · M-series chips'], ['x64', 'Intel', 'DMG · Intel processors']],
      methods: [['arm64', 'DMG · Apple Silicon'], ['x64', 'DMG · Intel'], ['quarantine', 'macOS · Allow opening'], ['homebrew', 'Homebrew']],
    },
    windows: {
      name: 'Windows', note: 'For x64 PCs. Choose your preferred installer.',
      files: [['exe', 'Windows installer', 'EXE · Current user'], ['msi', 'Windows MSI', 'MSI · Windows Installer']],
      methods: [['exe', 'EXE · Install download'], ['winget', 'WinGet'], ['chocolatey', 'Chocolatey']],
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
  const installers = [
    ['macos', 'arm64', 'macOS · Apple Silicon'],
    ['macos', 'x64', 'macOS · Intel'],
    ['windows', 'exe', 'Windows · EXE'],
    ['linux', 'deb', 'Linux · DEB'],
    ['linux', 'rpm', 'Linux · RPM'],
    ['linux', 'appimage', 'Linux · AppImage'],
  ];
  let installer = installers.find(([os]) => os === platform);
  function https(value) {
    try { const url = new URL(value); return url.protocol === 'https:' ? url.href : null; } catch { return null; }
  }
  function assetFilename(key) {
    const configured = config.downloads?.[platform];
    const url = https(typeof configured === 'string' ? configured : configured?.[key]);
    if (!url) return null;
    const filename = new URL(url).pathname.split('/').pop();
    // Only inert filenames may be inserted into copyable shell commands.
    return /^[a-zA-Z0-9][a-zA-Z0-9_.-]*$/.test(filename) ? filename : null;
  }
  function renderCommand(key, label) {
    let command = '';
    let note = '';
    let preview = false;
    if (platform === 'macos' && key === 'quarantine') {
      command = 'xattr -dr com.apple.quarantine "/Applications/Rowcraft.app"\nopen "/Applications/Rowcraft.app"';
      note = 'First drag Rowcraft from the DMG into Applications. If macOS blocks opening this unsigned build, run these commands only if you trust the download. They remove quarantine from Rowcraft only.';
    } else if (platform === 'macos' && (key === 'arm64' || key === 'x64')) {
      const filename = assetFilename(key);
      preview = !filename;
      command = filename ? `open "$HOME/Downloads/${filename}"` : '# DMG download is not published yet.';
      note = 'Download the matching DMG above into Downloads, then run this command. Drag Rowcraft into Applications and open it. If macOS blocks it, use the “macOS · Allow opening” commands below.';
    } else if (platform === 'windows' && key === 'exe') {
      const filename = assetFilename(key);
      preview = !filename;
      command = filename ? `Set-Location "$HOME\\Downloads"\nStart-Process -FilePath ".\\${filename}" -Wait` : '# EXE download is not published yet.';
      note = 'Download the EXE above into Downloads, then run in PowerShell and follow the installer. WebView2 downloads automatically if missing; internet access is required for that step.';
    } else if (platform === 'linux') {
      const filename = assetFilename(key);
      preview = !filename;
      const commands = {
        deb: `sudo apt install "./${filename}"`,
        rpm: `sudo dnf install "./${filename}"`,
        appimage: `chmod +x "./${filename}"\n"./${filename}"`,
      };
      command = filename ? `cd "$HOME/Downloads"\n${commands[key]}` : '# Package download is not published yet.';
      note = `Download the ${key === 'appimage' ? 'AppImage' : key.toUpperCase()} above into Downloads, then run these commands. DEB/RPM packages install their declared dependencies.`;
      if (key === 'appimage') note += ' Install lsof using your distribution’s package manager; FUSE 2 may also be required. If FUSE is unavailable, run the AppImage with --appimage-extract-and-run.';
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
    const section = document.createElement('section');
    section.className = 'setup-method';
    const heading = document.createElement('h4');
    heading.textContent = label;
    const description = document.createElement('p');
    description.textContent = note;
    const terminal = document.createElement('div');
    terminal.className = 'terminal-window';
    terminal.innerHTML = '<div class="terminal-heading"><span></span></div><div class="copy-command"><pre><code></code></pre></div><div class="terminal-footer"><span></span><button type="button"><svg class="icon" aria-hidden="true"><use href="#copy"/></svg><span></span></button></div>';
    terminal.querySelector('.terminal-heading span').textContent = platform === 'windows' ? 'PowerShell' : 'Terminal';
    const code = terminal.querySelector('code');
    code.textContent = preview ? `# Preview only. Requires a published package.\n${command}` : command;
    const state = terminal.querySelector('.terminal-footer > span');
    state.textContent = preview ? 'Command preview' : key === 'quarantine' ? 'If macOS blocks opening' : 'Ready to install';
    state.classList.toggle('is-preview', preview);
    const copy = terminal.querySelector('button');
    copy.querySelector('span').textContent = preview ? 'Copy example' : 'Copy command';
    copy.setAttribute('aria-label', `${preview ? 'Copy example' : 'Copy command'}: ${label}`);
    const feedback = document.createElement('p');
    feedback.className = 'install-feedback';
    feedback.setAttribute('role', 'status');
    feedback.setAttribute('aria-live', 'polite');
    copy.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(code.textContent);
        feedback.textContent = 'Commands copied.';
      } catch { feedback.textContent = 'Clipboard unavailable. Select the commands and copy them manually.'; }
    });
    section.append(heading, description, terminal, feedback);
    return section;
  }
  function renderPlatform() {
    const info = platforms[platform];
    const platformKeys = Object.keys(platforms);
    query('#platform-tabs').querySelectorAll('[role="tab"]').forEach((tab, index) => {
      const active = platformKeys[index] === platform;
      tab.setAttribute('aria-selected', String(active));
      tab.tabIndex = active ? 0 : -1;
    });
    query('#platform-panel').setAttribute('aria-labelledby', `platform-tab-${platformKeys.indexOf(platform)}`);
    const options = installers.filter(([os]) => os === platform);
    renderTabs('#installer-tabs', options.map(entry => platforms[platform].files.find(([key]) => key === entry[1])[1]),
      options.indexOf(installer), 'installer-tab', 'installer-panel', index => {
        installer = options[index];
        renderPlatform();
      });
    query('#installer-panel').setAttribute('aria-labelledby', `installer-tab-${options.indexOf(installer)}`);
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
    query('#setup-commands').replaceChildren(...info.methods.filter(([key]) => key === installer[1] || (platform === 'macos' && key === 'quarantine')).map(([key, label]) => renderCommand(key, label)));
  }
  function renderTabs(selector, labels, selected, prefix, panel, select) {
    query(selector).replaceChildren(...labels.map((label, index) => {
      const tab = document.createElement('button');
      tab.type = 'button';
      tab.id = `${prefix}-${index}`;
      tab.textContent = label;
      tab.setAttribute('role', 'tab');
      tab.setAttribute('aria-controls', panel);
      tab.setAttribute('aria-selected', String(index === selected));
      tab.tabIndex = index === selected ? 0 : -1;
      tab.addEventListener('click', () => {
        select(index);
        query(`#${prefix}-${index}`).focus();
      });
      tab.addEventListener('keydown', event => {
        const next = event.key === 'ArrowRight' ? (index + 1) % labels.length
          : event.key === 'ArrowLeft' ? (index + labels.length - 1) % labels.length
          : event.key === 'Home' ? 0 : event.key === 'End' ? labels.length - 1 : null;
        if (next === null) return;
        event.preventDefault();
        query(selector).children[next].click();
      });
      return tab;
    }));
  }
  const platformKeys = Object.keys(platforms);
  const choices = {};
  renderTabs('#platform-tabs', platformKeys.map(key => platforms[key].name), platformKeys.indexOf(platform),
    'platform-tab', 'platform-panel', index => {
      choices[platform] = installer;
      platform = platformKeys[index];
      installer = choices[platform] || installers.find(([os]) => os === platform);
      renderPlatform();
    });
  renderPlatform();
  query('#installer').hidden = false;
})();
