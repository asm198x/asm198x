'use strict';
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { createHash } = require('node:crypto');
const { versionOf, verifyChecksum, runChecked } = require('../.github/actions/setup/index.cjs');
const targets = ['aarch64-apple-darwin', 'x86_64-apple-darwin', 'x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu'];
const sha = data => createHash('sha256').update(data).digest('hex');
const archiveName = target => `asm198x-${target}.${target.includes('windows') ? 'zip' : 'tar.xz'}`;
function packageIdentity(plan, target, allTargets = true) {
  assert(targets.includes(target), 'unsupported target');
  assert.equal(plan.releases?.length, 1, 'expected one release');
  const release = plan.releases[0];
  assert.equal(release.app_name, 'asm198x');
  const version = versionOf(release.app_version);
  for (const expected of allTargets ? targets : [target]) {
    const name = archiveName(expected);
    assert(release.artifacts.includes(name), `plan omits ${expected}`);
    assert.deepEqual(plan.artifacts[name]?.target_triples, [expected], `wrong planned target ${expected}`);
    assert.equal(plan.artifacts[name]?.checksum, name + '.sha256');
  }
  return { version, archive: archiveName(target), target };
}
function nativeTarget(platform = process.platform, arch = process.arch) {
  const target = { 'darwin-arm64': targets[0], 'darwin-x64': targets[1], 'win32-x64': targets[2], 'linux-x64': targets[3], 'linux-arm64': targets[4] }[`${platform}-${arch}`];
  assert(target, `unsupported native runner ${platform}/${arch}`);
  return target;
}
function binaryTarget(data) {
  if (data.subarray(0, 4).equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46]))) {
    assert.equal(data[4], 2); assert.equal(data[5], 1);
    return { 62: targets[3], 183: targets[4] }[data.readUInt16LE(18)];
  }
  if (data.readUInt32LE(0) === 0xfeedfacf) return { 0x100000c: targets[0], 0x1000007: targets[1] }[data.readUInt32LE(4)];
  if (data.subarray(0, 2).toString() === 'MZ') {
    const offset = data.readUInt32LE(0x3c);
    assert.equal(data.subarray(offset, offset + 4).toString(), 'PE\0\0');
    if (data.readUInt16LE(offset + 4) === 0x8664) return targets[2];
  }
  throw new Error('unknown or unsupported executable architecture');
}
function archiveIdentity(plan, target, dist, allTargets = true) {
  const identity = packageIdentity(plan, target, allTargets);
  const archivePath = path.join(dist, identity.archive);
  const archive = fs.readFileSync(archivePath);
  verifyChecksum(archive, fs.readFileSync(archivePath + '.sha256', 'utf8'), identity.archive);
  const member = target.includes('windows') ? 'asm198x.exe' : `asm198x-${target}/asm198x`;
  const tar = process.platform === 'win32' ? path.join(process.env.SystemRoot, 'System32', 'tar.exe') : 'tar';
  const executable = runChecked(tar, ['-xOf', path.basename(archivePath), member], { cwd: path.dirname(archivePath), timeout: 30000, maxBuffer: 128 * 1024 * 1024 });
  assert.equal(binaryTarget(executable), target, 'archive contains wrong architecture');
  return { ...identity, archive_sha256: sha(archive), executable_sha256: sha(executable), executable };
}
module.exports = { targets, sha, archiveName, packageIdentity, nativeTarget, binaryTarget, archiveIdentity };
