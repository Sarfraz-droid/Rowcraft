import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

function prepare(env) {
  const directory = mkdtempSync(join(tmpdir(), 'rowcraft-signing-'));
  try {
    const output = join(directory, 'github-env');
    const result = spawnSync(process.execPath, [fileURLToPath(new URL('./prepare-macos-signing.mjs', import.meta.url))], {
      env: { ...env, GITHUB_ENV: output }, encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr);
    const contents = readFileSync(output, 'utf8');
    const entries = [...contents.matchAll(/(APPLE_\w+)<<(SIGNING_[\w-]+)\n([\s\S]*?)\n\2\n/g)];
    return Object.fromEntries(entries.map(([, name, , value]) => [name, value]));
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

test('absent secrets export only the ad-hoc identity', () => {
  assert.deepEqual(prepare({}), { APPLE_SIGNING_IDENTITY: '-' });
});

test('empty GitHub secrets do not trigger certificate import or notarization', () => {
  assert.deepEqual(prepare({ APPLE_CERTIFICATE: '', APPLE_CERTIFICATE_PASSWORD: '', APPLE_SIGNING_IDENTITY: '', APPLE_ID: '', APPLE_PASSWORD: '', APPLE_TEAM_ID: '' }), { APPLE_SIGNING_IDENTITY: '-' });
});

test('configured credentials survive GitHub environment file encoding', () => {
  const env = { APPLE_CERTIFICATE: 'base64-line-one\nbase64-line-two', APPLE_CERTIFICATE_PASSWORD: 'export-password', APPLE_SIGNING_IDENTITY: 'Developer ID Application: Example', APPLE_ID: 'example@example.com', APPLE_PASSWORD: 'app-password', APPLE_TEAM_ID: 'example-team' };
  assert.deepEqual(prepare(env), env);
});

test('passwordless certificates retain the empty password required by Tauri', () => {
  assert.deepEqual(prepare({ APPLE_CERTIFICATE: 'certificate', APPLE_CERTIFICATE_PASSWORD: '' }), { APPLE_SIGNING_IDENTITY: '-', APPLE_CERTIFICATE: 'certificate', APPLE_CERTIFICATE_PASSWORD: '' });
});
