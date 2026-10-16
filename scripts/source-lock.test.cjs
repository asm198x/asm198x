'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { spawnSync } = require('node:child_process');
const { sourceIdentity, guardedCommand, verifyReceipt } = require('./build-locked-package.cjs');
const { lockBuild, buildCommand, guardedBuild } = require('./generate-release-workflow.cjs');
const { sha, packageIdentity, targets, archiveName } = require('./package-identity.cjs');
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'asm198x-lock-control-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const git = args => {
    const result = spawnSync('git', ['-c', 'core.fsmonitor=false', ...args], { cwd: root, encoding: 'utf8', timeout: 10000 });
    assert.ifError(result.error); assert.equal(result.status, 0, result.stderr); return result.stdout;
  };
  git(['init', '--quiet']);
  fs.writeFileSync(path.join(root, 'Cargo.lock'), 'version = 4\n[[package]]\nname = "fixture"\nversion = "1.0.0"\n');
  git(['add', 'Cargo.lock']);
  // This disposable fixture has no signer; ordinary commit hooks still run.
  git(['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '--quiet', '-m', 'Fixture lock']);
  return root;
}
const command = script => [process.execPath, '-e', script];
test('the packaging boundary verifies the lock against Git on both sides', t => {
  const root = fixture(t); const expected = sourceIdentity(root);
  const manifest = path.join(root, 'manifest.json');
  const actual = guardedCommand(root, command('process.stdout.write(JSON.stringify({built:true}))'), manifest);
  assert.deepEqual(actual, expected);
  assert.deepEqual(JSON.parse(fs.readFileSync(manifest)), { built: true });
  assert.equal(actual.cargo_lock_sha256, sha(fs.readFileSync(path.join(root, 'Cargo.lock'))));
});
for (const mutation of ['changed', 'missing', 'empty']) {
  test(`${mutation} lock is rejected before the packager runs`, t => {
    const root = fixture(t);const lock = path.join(root, 'Cargo.lock');
    if (mutation === 'missing') fs.unlinkSync(lock);
    else fs.writeFileSync(lock, mutation === 'empty' ? '' : 'wrong');
    assert.throws(() => guardedCommand(root, command("require('node:fs').writeFileSync('executed','yes')"), path.join(root, 'manifest.json')));
    assert(!fs.existsSync(path.join(root, 'executed')));
  });
  test(`${mutation} lock during packaging rejects a successful command`, t => {
    const root = fixture(t);
    const action = mutation === 'missing' ? "fs.unlinkSync('Cargo.lock')" : `fs.writeFileSync('Cargo.lock', ${JSON.stringify(mutation === 'empty' ? '' : 'wrong')})`;
    assert.throws(() => guardedCommand(root, command(`const fs=require('node:fs');${action};process.stdout.write('{}')`), path.join(root, 'manifest.json')));
  });
}
test('a failed packaging command cannot produce a success receipt', t => {
  const root = fixture(t);
  assert.throws(() => guardedCommand(root, command('process.exit(9)'), path.join(root, 'manifest.json')), /packaging command failed/);
});
test('every receipt identity is checked against independent source and artifact observations', t => {
  const root = fixture(t);const source = sourceIdentity(root);
  const artifact = { target: 'aarch64-apple-darwin', archive: 'archive.tar.xz', archive_sha256: sha(Buffer.from('archive')), executable_sha256: sha(Buffer.from('executable')), version: '1.0.0' };
  const receipt = { schema_version: 1, kind: 'producer-source-lock', signed: false, source_commit: source.commit,
    committed_lock_sha256: source.cargo_lock_sha256, before_lock_sha256: source.cargo_lock_sha256, after_lock_sha256: source.cargo_lock_sha256,
    ...artifact, build: { job: 'build', run_id: '123', run_attempt: '2' }, compiled_dependency_graph: 'unknown' };
  const env = { GITHUB_RUN_ID: '123', GITHUB_RUN_ATTEMPT: '2' };
  verifyReceipt(receipt, artifact, source, env);
  for (const key of Object.keys(receipt)) {
    const missing = structuredClone(receipt); delete missing[key];
    assert.throws(() => verifyReceipt(missing, artifact, source, env), `missing ${key}`);
    const changed = structuredClone(receipt);changed[key] = 'wrong';
    assert.throws(() => verifyReceipt(changed, artifact, source, env), `changed ${key}`);
  }
  for (const key of ['job', 'run_id', 'run_attempt']) {
    const changed = structuredClone(receipt);changed.build[key] = '';
    assert.throws(() => verifyReceipt(changed, artifact, source, env));
  }
  assert.throws(() => verifyReceipt(null, artifact, source, env));
  assert.throws(() => verifyReceipt(receipt, artifact, source, { ...env, GITHUB_RUN_ID: '124' }), /producer run differs/);
  assert.throws(() => verifyReceipt(receipt, artifact, source, { ...env, GITHUB_RUN_ATTEMPT: '1' }), /producer attempt differs/);
});
test('generation requires the lock guard and receipt upload exactly once', () => {
  const text = fs.readFileSync(path.join(__dirname, '../.github/workflows/asm198x-release.yml'), 'utf8').replaceAll('\r\n', '\n');
  assert.equal(text.split(guardedBuild).length, 2);
  const generated = text.replace(guardedBuild, buildCommand)
    .replace("      - uses: actions/setup-node@v7\n        with:\n          node-version: '24'\n", '')
    .replace("\n            target/distrib/asm198x-${{ join(matrix.targets, ',') }}-source-lock.json", '');
  assert.equal(lockBuild(generated), text);
  assert.throws(() => lockBuild(generated.replace(buildCommand, 'changed upstream build')));
  assert.throws(() => lockBuild(generated + '\n' + buildCommand));
  assert.throws(() => lockBuild(text));
});
test('native PR and reusable candidates cannot silently use artifact-only evidence', () => {
  const text = fs.readFileSync(path.join(__dirname, '../.github/workflows/test-linux-packages.yml'), 'utf8').replaceAll('\r\n', '\n');
  assert(text.includes('node scripts/build-locked-package.cjs "$PACKAGE_TARGET"'));
  assert(text.includes('receipt_flags=(--require-source-lock)'));
  assert(text.includes('if [ -n "$PUBLISHED_VERSION" ]; then\n            receipt_flags=(--published-source)'));
  for (const name of ['report.json', 'wrong-bytes.json', 'corrupt-archive.json']) {
    const line = text.split('\n').find(line => line.includes('node scripts/test-candidate-package.cjs') && line.includes(name));
    assert(line.includes('"${receipt_flags[@]}"'));
  }
});

test('producer manifests validate their selected target while qualification requires the full set', () => {
  for (const target of targets) {
    const archive = archiveName(target);
    const local = { releases: [{app_name: 'asm198x', app_version: '1.0.0', artifacts: [archive]}], artifacts: { [archive]: { target_triples: [target], checksum: archive + '.sha256' } } };
    assert.equal(packageIdentity(local, target, false).target, target);
    assert.throws(() => packageIdentity(local, target), /plan omits/);
    const wrong = targets.find(other => other !== target);
    assert.throws(() => packageIdentity(local, wrong, false), /plan omits/);
    local.artifacts[archive].target_triples = [wrong];
    assert.throws(() => packageIdentity(local, target, false), /wrong planned target/);
  }
});
