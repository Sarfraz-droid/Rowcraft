// Builds the `rowcraft` CLI and stages it where Tauri bundles it as an external binary
// (`bundle.externalBin` = "binaries/rowcraft"), so every installer ships the CLI next to the app.
// Runs from `build.beforeBuildCommand`; Tauri passes the bundle target in TAURI_ENV_TARGET_TRIPLE.
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const triple =
  process.env.TAURI_ENV_TARGET_TRIPLE ||
  /^host: (.+)$/m.exec(execFileSync('rustc', ['-vV'], { encoding: 'utf8' }))?.[1];
if (!triple) throw new Error('Cannot determine the target triple; set TAURI_ENV_TARGET_TRIPLE.');

const exe = triple.includes('windows') ? '.exe' : '';
const targetDir = process.env.CARGO_TARGET_DIR
  ? resolve(process.env.CARGO_TARGET_DIR)
  : join(root, 'target');

console.log(`Building rowcraft CLI for ${triple}`);
execFileSync(
  'cargo',
  ['build', '--release', '--locked', '-p', 'rowcraft-cli', '--target', triple],
  { cwd: root, stdio: 'inherit' },
);

const dest = join(root, 'apps/rowcraft-desktop/src-tauri/binaries', `rowcraft-${triple}${exe}`);
mkdirSync(dirname(dest), { recursive: true });
copyFileSync(join(targetDir, triple, 'release', `rowcraft${exe}`), dest);
console.log(`Staged ${dest}`);
