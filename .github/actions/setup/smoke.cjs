'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

assert.equal(process.env.INSTALLED_VERSION, '0.0.57');
assert.equal(process.env.INSTALLED_TARGET, process.env.EXPECTED_TARGET);
const output = path.join(process.env.RUNNER_TEMP, 'asm198x-setup-smoke.bin');
const result = spawnSync('asm198x', ['--dialect', 'acme', path.join(__dirname, 'smoke.asm'), '-o', output], { encoding: 'utf8' });
assert.ifError(result.error);
assert.equal(result.status, 0, result.stderr);
assert.deepEqual(fs.readFileSync(output), Buffer.from([0xea, 0x60]));
console.log('Installed release is on PATH and assembles the expected bytes');
