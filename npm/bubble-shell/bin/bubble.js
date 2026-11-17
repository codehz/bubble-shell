#!/usr/bin/env node
const { spawnSync } = require('child_process');
const { existsSync } = require('fs');
const { join } = require('path');

const PLATFORM_PACKAGES = {
  'darwin-arm64': '@bubble-shell/darwin-arm64',
  'linux-x64': '@bubble-shell/linux-x64',
  'win32-x64': '@bubble-shell/windows-x64',
};

function binName() {
  return process.platform === 'win32' ? 'bubble.exe' : 'bubble';
}

function resolveBin(packageName) {
  const name = binName();
  try {
    const entry = require.resolve(packageName);
    const fromPackage = join(entry, '..', 'bin', name);
    if (existsSync(fromPackage)) return fromPackage;
  } catch {
    // Optional dependency is absent; fall back to the checkout layout.
  }
  const folder = packageName.slice('@bubble-shell/'.length);
  const beside = join(__dirname, '..', '..', '@bubble-shell', folder, 'bin', name);
  if (existsSync(beside)) return beside;
  return null;
}

function main() {
  const key = `${process.platform}-${process.arch}`;
  const packageName = PLATFORM_PACKAGES[key];
  if (!packageName) {
    console.error(`Unsupported platform: ${process.platform}-${process.arch}`);
    process.exit(1);
  }

  const binPath = resolveBin(packageName);
  if (!binPath) {
    console.error(`Failed to find ${packageName}. Please reinstall bubble-shell.`);
    process.exit(1);
  }

  const result = spawnSync(binPath, process.argv.slice(2), { stdio: 'inherit' });
  if (result.error) {
    console.error(`Failed to execute bubble: ${result.error.message}`);
    process.exit(1);
  }
  process.exit(result.status ?? 1);
}

main();
