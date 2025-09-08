#!/usr/bin/env node
const { spawn } = require('node:child_process');
const { join } = require('node:path');
const { existsSync } = require('node:fs');

const isWindows = process.platform === 'win32';
const exe = isWindows ? '.exe' : '';
const localPath = join(__dirname, '..', 'dist', 'bin', `parse${exe}`);

const bin = existsSync(localPath) ? localPath : 'parse';

const child = spawn(bin, process.argv.slice(2), { stdio: 'inherit', shell: isWindows });
child.on('exit', (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  process.exit(code ?? 1);
});

