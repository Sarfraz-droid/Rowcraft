/* Release assets are matched by platform; filenames are never invented. */
((root) => {
  function githubRelease(release) {
    const downloads = { macos: {}, windows: {}, linux: {} };
    for (const asset of release.assets || []) {
      const name = asset.name || '';
      let url;
      try { url = new URL(asset.browser_download_url); } catch { continue; }
      if (url.protocol !== 'https:') continue;
      if (/\.dmg$/i.test(name)) {
        if (/(?:aarch64|arm64)/i.test(name)) downloads.macos.arm64 = url.href;
        else if (/(?:x64|x86_64|amd64)/i.test(name)) downloads.macos.x64 = url.href;
      } else if (/(?:x64|x86_64|amd64)/i.test(name)) {
        if (/\.exe$/i.test(name)) downloads.windows.exe = url.href;
        else if (/\.msi$/i.test(name)) downloads.windows.msi = url.href;
        else if (/\.appimage$/i.test(name)) downloads.linux.appimage = url.href;
        else if (/\.deb$/i.test(name)) downloads.linux.deb = url.href;
        else if (/\.rpm$/i.test(name)) downloads.linux.rpm = url.href;
      }
    }
    return {
      tag: release.tag_name,
      version: release.tag_name.replace(/^v(?=\d)/, ''),
      prerelease: Boolean(release.prerelease),
      url: release.html_url,
      downloads,
    };
  }

  function catalog(releases, latestTag) {
    const unique = new Map();
    for (const release of releases) {
      if (!release.draft && typeof release.tag_name === 'string' && release.tag_name.trim()) {
        unique.set(release.tag_name, githubRelease(release));
      }
    }
    const versions = [...unique.values()];
    const latest = versions.find(release => release.tag === latestTag && !release.prerelease)
      || versions.find(release => !release.prerelease);
    if (!latest) throw new Error('No stable releases are published yet.');
    return { latestTag: latest.tag, releases: versions };
  }

  async function load(repositoryUrl, fetcher = fetch) {
    const repository = new URL(repositoryUrl);
    if (repository.origin !== 'https://github.com' || !/^\/[\w.-]+\/[\w.-]+\/?$/.test(repository.pathname)) {
      throw new Error('A GitHub repository is required for release updates.');
    }
    const endpoint = `https://api.github.com/repos${repository.pathname.replace(/\/$/, '')}/releases`;
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 10000);
    const get = url => fetcher(url, { signal: controller.signal, headers: { Accept: 'application/vnd.github+json' } });
    try {
      const [first, latest] = await Promise.all([get(`${endpoint}?per_page=100`), get(`${endpoint}/latest`)]);
      if (!first.ok || !latest.ok) throw new Error('Release updates are unavailable.');
      const versions = await first.json();
      const marker = await latest.json();
      let response = first;
      const visited = new Set();
      while (true) {
        const next = response.headers.get('Link')?.match(/<([^>]+)>;\s*rel="next"/)?.[1];
        if (!next || visited.has(next)) break;
        const url = new URL(next);
        if (url.origin !== 'https://api.github.com' || url.pathname !== new URL(endpoint).pathname) break;
        visited.add(next);
        response = await get(url.href);
        if (!response.ok) throw new Error('Release updates are unavailable.');
        versions.push(...await response.json());
      }
      return catalog(versions, marker.tag_name);
    } finally {
      clearTimeout(timeout);
    }
  }

  const api = { githubRelease, catalog, load };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.RowcraftReleases = api;
})(globalThis);
