const { test } = require('node:test');
const assert = require('node:assert/strict');
const { catalog, load } = require('../releases.js');
const release = (tag, extra = {}) => ({ tag_name: tag, html_url: `https://github.com/example/app/releases/tag/${tag}`, assets: [], ...extra });
const response = (data, link = '') => ({ ok: true, json: async () => data, headers: { get: () => link } });

test('GitHub latest marker controls the default even when another tag has a larger version', () => {
  const result = catalog([release('0.0.3'), release('v0.0.1'), release('draft', { draft: true })], 'v0.0.1');
  assert.equal(result.latestTag, 'v0.0.1');
  assert.deepEqual(result.releases.map(item => item.version), ['0.0.3', '0.0.1']);
});

test('pre-releases are listed but never become the default stable release', () => {
  const result = catalog([release('v2-beta', { prerelease: true }), release('v1.0.0')], 'v2-beta');
  assert.equal(result.latestTag, 'v1.0.0');
  assert.equal(result.releases[0].prerelease, true);
});

test('downloads use published assets, ignore unsafe URLs, and keep missing formats unavailable', () => {
  const names = ['Rowcraft_1_aarch64.dmg', 'Rowcraft_1_x64.dmg', 'Rowcraft_1_x64-setup.exe', 'Rowcraft_1_amd64.deb', 'Rowcraft-1.x86_64.rpm', 'Rowcraft_1_amd64.AppImage', 'Rowcraft_1_arm64.deb'];
  const assets = names.map(name => ({ name, browser_download_url: `https://example.com/${name}` }));
  assets.push({ name: 'Rowcraft_1_x64.msi', browser_download_url: 'javascript:alert(1)' });
  const result = catalog([release('v1.0.0', { assets }), release('v0.9.0')], 'v1.0.0');
  const downloads = result.releases[0].downloads;
  assert.ok(downloads.macos.arm64.endsWith('aarch64.dmg'));
  assert.ok(downloads.macos.x64.endsWith('x64.dmg'));
  assert.ok(downloads.windows.exe.endsWith('setup.exe'));
  assert.equal(downloads.windows.msi, undefined);
  assert.ok(downloads.linux.deb.endsWith('amd64.deb'));
  assert.ok(downloads.linux.rpm.endsWith('x86_64.rpm'));
  assert.ok(downloads.linux.appimage.endsWith('.AppImage'));
  assert.deepEqual(result.releases[1].downloads, { macos: {}, windows: {}, linux: {} });
});

test('release loading follows pagination and uses the latest endpoint', async () => {
  const endpoint = 'https://api.github.com/repos/example/app/releases';
  const calls = [];
  const fetcher = async url => {
    calls.push(url);
    if (url.endsWith('/latest')) return response(release('v2.0.0'));
    if (url.includes('page=2')) return response([release('v1.0.0')]);
    return response([release('v2.0.0')], `<${endpoint}?page=2>; rel="next"`);
  };
  const result = await load('https://github.com/example/app', fetcher);
  assert.equal(result.latestTag, 'v2.0.0');
  assert.equal(result.releases.length, 2);
  assert.equal(calls.length, 3);
});

test('rate limits and invalid release responses fail cleanly for the saved catalog fallback', async () => {
  await assert.rejects(load('https://github.com/example/app', async () => ({ ok: false })), /unavailable/);
  await assert.rejects(load('https://untrusted.example/app'), /GitHub/);
  assert.throws(() => catalog([], 'v1.0.0'), /No stable releases/);
});
