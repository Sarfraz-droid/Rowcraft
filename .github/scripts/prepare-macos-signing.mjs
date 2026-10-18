import { appendFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';

// Tauri checks whether these variables exist, including when their values are empty.
// Only export certificate credentials when a certificate was actually provided.
const certificate = process.env.APPLE_CERTIFICATE ?? '';
const values = {
  APPLE_SIGNING_IDENTITY: process.env.APPLE_SIGNING_IDENTITY?.trim() || '-',
};
if (certificate.trim()) {
  values.APPLE_CERTIFICATE = certificate;
  // A passwordless .p12 still needs this variable to exist for Tauri's import path.
  values.APPLE_CERTIFICATE_PASSWORD = process.env.APPLE_CERTIFICATE_PASSWORD ?? '';
}
for (const name of ['APPLE_ID', 'APPLE_PASSWORD', 'APPLE_TEAM_ID']) {
  if (process.env[name]?.trim()) values[name] = process.env[name];
}

const output = Object.entries(values).map(([name, value]) => {
  const delimiter = `SIGNING_${randomUUID()}`;
  return `${name}<<${delimiter}\n${value}\n${delimiter}\n`;
}).join('');
appendFileSync(process.env.GITHUB_ENV, output);
console.log(certificate.trim() ? 'macOS certificate signing configured.' : 'macOS ad-hoc signing configured; no certificate will be imported.');
