'use strict';
// Unsigned producer evidence: this does not identify a binary's compiled graph.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { sha, archiveIdentity, nativeTarget } = require('./package-identity.cjs');
const { runChecked } = require('../.github/actions/setup/index.cjs');
function sourceIdentity(root) {
  const commit = runChecked('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8', timeout: 10000 }).trim();
  assert.match(commit, /^[a-f0-9]{40}$/, 'invalid checkout commit');
  const committed = runChecked('git', ['show', `${commit}:Cargo.lock`], { cwd: root, timeout: 10000, maxBuffer: 4 * 1024 * 1024 });
  assert(committed.length > 0, 'empty committed Cargo.lock');
  const current = fs.readFileSync(path.join(root, 'Cargo.lock'));
  assert(current.length > 0, 'empty producer Cargo.lock');
  assert.deepEqual(current, committed, 'producer Cargo.lock differs from checkout commit');
  return { commit, cargo_lock_sha256: sha(committed) };
}
function guardedCommand(root, argv, manifest) {
  const before = sourceIdentity(root);
  assert(argv.length > 0, 'missing packaging command');
  fs.mkdirSync(path.dirname(manifest), { recursive: true });
  const fd = fs.openSync(manifest, 'w');
  try {
    const result = spawnSync(argv[0], argv.slice(1), { cwd: root, stdio: ['ignore', fd, 'inherit'], timeout: 20 * 60 * 1000 });
    if (result.error) throw result.error;
    assert.equal(result.status, 0, 'packaging command failed');
  } finally { fs.closeSync(fd); }
  assert.deepEqual(sourceIdentity(root), before, 'producer source changed during packaging');
  return before;
}
function verifyReceipt(receipt, artifact, source, env = null) {
  assert.equal(receipt.schema_version, 1, 'source-lock receipt schema differs');
  assert.equal(receipt.kind, 'producer-source-lock', 'source-lock receipt kind differs');
  assert.equal(receipt.signed, false, 'receipt signature scope differs');
  assert.equal(receipt.compiled_dependency_graph, 'unknown', 'receipt compiled graph scope differs');
  assert.equal(receipt.source_commit, source.commit, 'receipt source commit differs');
  assert.match(source.cargo_lock_sha256, /^[a-f0-9]{64}$/, 'invalid expected lock hash');
  for (const key of ['committed_lock_sha256', 'before_lock_sha256', 'after_lock_sha256']) {
    assert.equal(receipt[key], source.cargo_lock_sha256, `receipt ${key} differs`);
  }
  for (const key of ['target', 'archive', 'archive_sha256', 'executable_sha256', 'version']) {
    assert.equal(receipt[key], artifact[key], `receipt ${key} differs`);
  }
  for (const key of ['job', 'run_id', 'run_attempt']) assert.equal(typeof receipt.build?.[key], 'string', `missing producer ${key}`);
  for (const value of Object.values(receipt.build)) assert(value.length > 0, 'empty producer build identity');
  if (env) {
    assert.equal(receipt.build.run_id, env.GITHUB_RUN_ID || 'local', 'receipt producer run differs');
    assert.equal(receipt.build.run_attempt, env.GITHUB_RUN_ATTEMPT || 'local', 'receipt producer attempt differs');
  }
}
function build(root, target, manifest, dist, argv, env = process.env) {
  assert.equal(nativeTarget(), target, 'packaging must use native target');
  const receiptPath = path.join(dist, `asm198x-${target}-source-lock.json`);
  assert(!fs.existsSync(receiptPath), 'stale producer receipt exists');
  const source = guardedCommand(root, argv, manifest);
  const artifact = archiveIdentity(JSON.parse(fs.readFileSync(manifest, 'utf8')), target, dist, false);
  const receipt = {
    schema_version: 1, kind: 'producer-source-lock', signed: false,
    source_commit: source.commit, committed_lock_sha256: source.cargo_lock_sha256,
    before_lock_sha256: source.cargo_lock_sha256, after_lock_sha256: source.cargo_lock_sha256,
    target, archive: artifact.archive, archive_sha256: artifact.archive_sha256,
    executable_sha256: artifact.executable_sha256, version: artifact.version,
    build: { job: env.GITHUB_JOB || 'local', run_id: env.GITHUB_RUN_ID || 'local', run_attempt: env.GITHUB_RUN_ATTEMPT || 'local' },
    command: argv, compiled_dependency_graph: 'unknown',
  };
  verifyReceipt(receipt, artifact, source, env);
  fs.writeFileSync(receiptPath, JSON.stringify(receipt, null, 2) + '\n', { flag: 'wx' });
  return receipt;
}
module.exports = { sourceIdentity, guardedCommand, verifyReceipt, build };
if (require.main === module) {
  try {
    const [target, manifest, dist, separator, ...argv] = process.argv.slice(2);
    assert.equal(separator, '--', 'usage: build-locked-package.cjs <target> <manifest> <distrib> -- <command> [args]');
    build(process.cwd(), target, path.resolve(manifest), path.resolve(dist), argv);
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
