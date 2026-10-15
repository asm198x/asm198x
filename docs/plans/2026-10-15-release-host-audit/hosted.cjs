'use strict';
// Use the runner's preinstalled Python 3.13+, without adding an installer dependency.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const candidates = ['python3.13', 'python3', 'python'];
const cache = path.join(process.env.RUNNER_TOOL_CACHE || '', 'Python');
if (fs.existsSync(cache)) {
  for (const version of fs.readdirSync(cache).filter(v => /^3\.(1[3-9]|[2-9][0-9])\.\d+$/.test(v)).sort().reverse()) {
    const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
    candidates.push(path.join(cache, version, arch, process.platform === 'win32' ? 'python.exe' : 'bin/python3'));
  }
}
const python = candidates.find(candidate => {
  const p = spawnSync(candidate, ['-c', 'import sys; sys.exit(not ((3,13) <= sys.version_info < (4,0)))'], { timeout: 10000 });
  return !p.error && p.status === 0;
});
assert(python, 'A preinstalled Python 3.13+ is required');
assert(process.env.RUNNER_TEMP && process.env.INSTALLED_TARGET && process.env.INSTALLED_PATH);
const result = spawnSync(python, [path.join(__dirname, 'probe.py'),
  '--target', process.env.INSTALLED_TARGET,
  '--binary', path.join(process.env.INSTALLED_PATH, process.platform === 'win32' ? 'asm198x.exe' : 'asm198x'),
  '--cache', path.join(process.env.RUNNER_TEMP, 'asm198x-release-cache'),
  '--out', path.join(process.env.RUNNER_TEMP, 'asm198x-release-report', `${process.env.INSTALLED_TARGET}.json`)],
  { stdio: 'inherit', timeout: 300000 });
assert.ifError(result.error);
assert.equal(result.status, 0, 'Audit infrastructure/positive control failed; capability findings are in the report');
