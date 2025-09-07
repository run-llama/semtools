#!/usr/bin/env node
/*
  Prefer zero-build: download prebuilt binaries from GitHub Releases.
  Fallback: build Rust binaries on npm install and place them in dist/bin.
*/

const { spawnSync } = require('node:child_process');
const { existsSync, mkdirSync, copyFileSync, chmodSync, createWriteStream } = require('node:fs');
const { join } = require('node:path');
const https = require('node:https');
const os = require('node:os');
const tar = require('tar');
const unzipper = require('unzipper');

function fail(message) {
  console.error(`@llamaindex/semtools install error: ${message}`);
  process.exit(1);
}

function hasCargo() {
  const result = spawnSync('cargo', ['--version'], { stdio: 'ignore' });
  return result.status === 0;
}

function buildBinaries() {
  const args = ['build', '--release', '--locked'];
  const result = spawnSync('cargo', args, { stdio: 'inherit' });
  if (result.status !== 0) {
    fail('`cargo build --release` failed. Please ensure Rust and Cargo are installed. See https://www.rust-lang.org/tools/install');
  }
}

function placeBinaries() {
  const isWindows = process.platform === 'win32';
  const exe = isWindows ? '.exe' : '';

  const builtParse = join(__dirname, '..', 'target', 'release', `parse${exe}`);
  const builtSearch = join(__dirname, '..', 'target', 'release', `search${exe}`);

  const destDir = join(__dirname, '..', 'dist', 'bin');
  mkdirSync(destDir, { recursive: true });

  let anyPlaced = false;

  if (existsSync(builtParse)) {
    const dest = join(destDir, `parse${exe}`);
    copyFileSync(builtParse, dest);
    try { chmodSync(dest, 0o755); } catch {}
    anyPlaced = true;
  }

  if (existsSync(builtSearch)) {
    const dest = join(destDir, `search${exe}`);
    copyFileSync(builtSearch, dest);
    try { chmodSync(dest, 0o755); } catch {}
    anyPlaced = true;
  }

  if (!anyPlaced) {
    fail('No binaries were produced. Expected to find target/release/parse and/or target/release/search.');
  }
}

function detectMusl() {
  try {
    // Node >= 12 has process.report
    if (typeof process.report?.getReport === 'function') {
      const glibc = process.report.getReport().header?.glibcVersionRuntime;
      return !glibc; // if no glibc, assume musl (e.g., Alpine)
    }
  } catch {}
  return false;
}

function detectTargetTriple() {
  const platform = os.platform();
  const arch = os.arch();

  if (platform === 'linux') {
    const musl = detectMusl();
    if (arch === 'x64') return musl ? 'x86_64-unknown-linux-musl' : 'x86_64-unknown-linux-gnu';
    if (arch === 'arm64') return musl ? 'aarch64-unknown-linux-musl' : 'aarch64-unknown-linux-gnu';
  } else if (platform === 'darwin') {
    if (arch === 'x64') return 'x86_64-apple-darwin';
    if (arch === 'arm64') return 'aarch64-apple-darwin';
  } else if (platform === 'win32') {
    if (arch === 'x64') return 'x86_64-pc-windows-msvc';
    if (arch === 'arm64') return 'aarch64-pc-windows-msvc';
  }
  return null;
}

function download(url, destPath) {
  return new Promise((resolve, reject) => {
    const file = createWriteStream(destPath);
    https.get(url, (res) => {
      if (res.statusCode && res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        // Follow redirect
        return resolve(download(res.headers.location, destPath));
      }
      if (res.statusCode !== 200) {
        return reject(new Error(`HTTP ${res.statusCode}`));
      }
      res.pipe(file);
      file.on('finish', () => file.close(resolve));
    }).on('error', (err) => {
      reject(err);
    });
  });
}

async function tryDownloadPrebuilt() {
  const version = require('../package.json').version;
  const target = detectTargetTriple();
  if (!target) return false;

  const isWindows = process.platform === 'win32';
  const assetExt = isWindows ? 'zip' : 'tar.gz';
  const assetName = `semtools-${target}.${assetExt}`;
  const downloadUrl = `https://github.com/run-llama/semtools/releases/download/v${version}/${assetName}`;

  const tmpDir = join(__dirname, '..', 'dist', 'tmp');
  const distBin = join(__dirname, '..', 'dist', 'bin');
  mkdirSync(tmpDir, { recursive: true });
  mkdirSync(distBin, { recursive: true });

  const archivePath = join(tmpDir, assetName);
  try {
    console.log(`@llamaindex/semtools: downloading prebuilt ${assetName} ...`);
    await download(downloadUrl, archivePath);
  } catch (e) {
    console.warn(`@llamaindex/semtools: prebuilt download failed: ${e.message}`);
    return false;
  }

  try {
    if (isWindows) {
      await unzipper.Open.file(archivePath).then(d => d.extract({ path: distBin, concurrency: 5 }));
    } else {
      await tar.x({ file: archivePath, cwd: distBin });
    }
    // Ensure executables
    try { chmodSync(join(distBin, 'parse'), 0o755); } catch {}
    try { chmodSync(join(distBin, 'search'), 0o755); } catch {}
    return true;
  } catch (e) {
    console.warn(`@llamaindex/semtools: failed to extract prebuilt: ${e.message}`);
    return false;
  }
}

async function main() {
  // Try prebuilt first
  const ok = await tryDownloadPrebuilt();
  if (ok) return;

  // Fallback to local build
  if (!hasCargo()) {
    fail('No prebuilt binary available and Cargo was not found. Install Rust or use a supported platform/arch.');
  }
  buildBinaries();
  placeBinaries();
}

main().catch(e => fail(e.message));


