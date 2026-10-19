'use strict';

const { cpSync, mkdirSync, rmSync } = require('node:fs');
const { resolve } = require('node:path');

require('./build-config.cjs');

const root = resolve(__dirname, '..');
const output = resolve(root, 'dist');
rmSync(output, { recursive: true, force: true });
mkdirSync(output, { recursive: true });

// Publish only browser assets; keep environment files and development tools private.
for (const file of [
  'index.html',
  'styles.css',
  'runtime-config.js',
  'config.js',
  'posthog-init.js',
  'analytics.js',
  'main.js',
  'benchmark-tabs.js',
  'releases.js',
  'downloads.js',
  'assets',
]) {
  cpSync(resolve(root, file), resolve(output, file), { recursive: true });
}

console.log('Static site built in dist/');
