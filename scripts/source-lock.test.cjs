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

for (const receiptStatus of [200, 404, 403, 500]) {
  test(`published receipt HTTP ${receiptStatus} preserves its actual evidence scope`, async t => {
    const { fetchPublished } = require('./fetch-published-package.cjs');
    const { nativeTarget } = require('./package-identity.cjs');
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'asm198x-public-lock-control-'));
    t.after(() => fs.rmSync(root, {recursive: true, force: true}));
    const target = nativeTarget();const archive = archiveName(target);
    const plan = { releases: [{ app_name: 'asm198x', app_version: '1.0.0', artifacts: targets.map(archiveName) }], artifacts: {} };
    for (const item of targets) plan.artifacts[archiveName(item)] = { target_triples: [item], checksum: archiveName(item) + '.sha256' };
    const lock = 'version = 4\n';const commit = 'a'.repeat(40);const requests = [];
    const fetcher = async url => {
      requests.push(url);
      if (url.endsWith('dist-manifest.json')) return new Response(JSON.stringify(plan));
      if (url.endsWith(archive)) return new Response('archive bytes');
      if (url.endsWith(archive + '.sha256')) return new Response('checksum bytes');
      if (url.endsWith('-source-lock.json')) return new Response(JSON.stringify({receipt: 'retained for later validation'}), {status: receiptStatus});
      if (url.includes('/commits/')) return new Response(JSON.stringify({sha: commit}));
      if (url.endsWith('/Cargo.lock')) { assert(url.includes(commit));return new Response(lock); }
      assert.fail(`unexpected request: ${url}`);
    };
    if (![200,404].includes(receiptStatus)) {
      await assert.rejects(fetchPublished('1.0.0', target, root, fetcher), /download .*source-lock.json/);
      return;
    }
    await fetchPublished('1.0.0', target, root, fetcher);
    const acquisition = JSON.parse(fs.readFileSync(path.join(root, 'acquisition.json')));
    assert.equal(acquisition.status, 'acquired');
    assert.equal(acquisition.receipt, receiptStatus === 200 ? 'present' : 'absent');
    const source = path.join(root, 'distrib', 'published-source.json');
    const receipt = path.join(root, 'distrib', `asm198x-${target}-source-lock.json`);
    assert.equal(fs.existsSync(source), receiptStatus === 200);
    assert.equal(fs.existsSync(receipt), receiptStatus === 200);
    if (receiptStatus === 200) {
      assert.deepEqual(JSON.parse(fs.readFileSync(source)), {commit, cargo_lock_sha256: sha(Buffer.from(lock))});
      assert.deepEqual(JSON.parse(fs.readFileSync(receipt)), {receipt: 'retained for later validation'});
    } else assert(!requests.some(url => url.includes('/commits/')));
  });
}

function publishedFixture(t, apiStatus = 200) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'asm198x-acquisition-control-'));
  t.after(() => fs.rmSync(root, {recursive: true, force: true}));
  const { nativeTarget } = require('./package-identity.cjs');
  const target = nativeTarget();const archive = archiveName(target);const requests = [];
  const plan = { releases: [{app_name: 'asm198x', app_version: '1.0.0', artifacts: targets.map(archiveName)}], artifacts: {} };
  for (const item of targets) plan.artifacts[archiveName(item)] = {target_triples: [item], checksum: archiveName(item) + '.sha256'};
  const token = 'fixture-read-only-token';
  const acquisitionPath = path.join(root, 'retained', 'acquisition.json');
  const fetcher = async (url, options) => {
    requests.push({url, options});
    if (url.endsWith('dist-manifest.json')) return new Response(JSON.stringify(plan));
    if (url.endsWith(archive) || url.endsWith(archive + '.sha256')) return new Response('fixture bytes');
    if (url.endsWith('-source-lock.json')) return new Response('{}');
    if (url.includes('/commits/')) return new Response(JSON.stringify({sha: 'a'.repeat(40), message: token}), {
      status: apiStatus, headers: {'x-ratelimit-remaining': '0', 'x-ratelimit-reset': '1800000000', 'x-github-request-id': 'ABC:123', 'x-not-retained': token},
    });
    if (url.endsWith('/Cargo.lock')) return new Response('version = 4\n');
    assert.fail(`unexpected request: ${url}`);
  };
  return {root, target, token, acquisitionPath, requests, fetcher};
}

test('published lookup authenticates only the API and retains acquired status', async t => {
  const f = publishedFixture(t);const {fetchPublished} = require('./fetch-published-package.cjs');
  await fetchPublished('1.0.0', f.target, f.root, f.fetcher, {token: f.token, acquisitionPath: f.acquisitionPath});
  for (const {url, options} of f.requests) {
    const api = new URL(url).hostname === 'api.github.com';
    assert.equal(new Headers(options.headers).get('authorization'), api ? `Bearer ${f.token}` : null);
    if (api) assert.equal(options.redirect, 'error', 'authenticated API must not follow redirects');
  }
  const record = JSON.parse(fs.readFileSync(f.acquisitionPath));
  assert.equal(record.status, 'acquired');assert.equal(record.kind, 'published-release');
  assert.equal(record.published_version, '1.0.0');assert.equal(record.target, f.target);
  assert(!fs.readFileSync(f.acquisitionPath, 'utf8').includes(f.token));
});

for (const status of [403, 429, 500]) {
  test(`source lookup HTTP ${status} retains a failed record without qualifying or leaking credentials`, async t => {
    const f = publishedFixture(t, status);const {fetchPublished} = require('./fetch-published-package.cjs');
    await assert.rejects(fetchPublished('1.0.0', f.target, f.root, f.fetcher, {token: f.token, acquisitionPath: f.acquisitionPath}), new RegExp(`published source commit: ${status}`));
    const raw = fs.readFileSync(f.acquisitionPath, 'utf8');const record = JSON.parse(raw);
    assert.equal(record.status, 'failed');assert.equal(record.qualified, false);assert.equal(record.stage, 'source_commit');
    assert.deepEqual(record.http, {status, headers: {'x-ratelimit-remaining': '0', 'x-ratelimit-reset': '1800000000', 'x-github-request-id': 'ABC:123'}});
    assert(!raw.includes(f.token));assert(!raw.includes('x-not-retained'));
    assert(!f.requests.some(r => r.url.endsWith('/Cargo.lock')));
    assert(!fs.existsSync(path.join(f.root, 'distrib', 'published-source.json')));
  });
}

test('early acquisition errors retain a nonzero failure outcome and redact error text', async t => {
  const f = publishedFixture(t);const {fetchPublished} = require('./fetch-published-package.cjs');
  const fetcher = async () => { throw new Error(`network error ${f.token}`); };
  await assert.rejects(fetchPublished('1.0.0', f.target, f.root, fetcher, {token: f.token, acquisitionPath: f.acquisitionPath}), error => !error.message.includes(f.token));
  const raw = fs.readFileSync(f.acquisitionPath, 'utf8');const record = JSON.parse(raw);
  assert.equal(record.status, 'failed');assert.equal(record.stage, 'manifest');assert.equal(record.qualified, false);
  assert(!raw.includes(f.token));
});

test('the CLI exits unsuccessfully and retains acquisition failure before qualification', t => {
  const f = publishedFixture(t);
  const preload = path.join(f.root, 'refuse-fetch.cjs');
  fs.writeFileSync(preload, "global.fetch = async () => new Response('not retained', {status:403,headers:{'x-ratelimit-remaining':'0'}});\n");
  const result = spawnSync(process.execPath, ['--require', preload, path.join(__dirname, 'fetch-published-package.cjs'), '1.0.0', f.target, f.root, f.acquisitionPath], {
    encoding: 'utf8', timeout: 10000, env: {...process.env, GH_TOKEN: f.token},
  });
  assert.ifError(result.error);assert.equal(result.status, 1);assert.equal(result.signal, null);
  assert(result.stderr.includes('download dist-manifest.json: 403'));
  assert(!result.stderr.includes(f.token));
  const record = JSON.parse(fs.readFileSync(f.acquisitionPath));
  assert.equal(record.status, 'failed');assert.equal(record.qualified, false);
  assert(!fs.existsSync(path.join(f.root, 'candidate-plan.json')));
});

test('published workflow scopes its existing token and retains acquisition before qualification', () => {
  const text = fs.readFileSync(path.join(__dirname, '../.github/workflows/test-linux-packages.yml'), 'utf8').replaceAll('\r\n', '\n');
  const acquisition = text.slice(text.indexOf('      - name: Download published archives'), text.indexOf('      - name: Fetch actual candidate packages'));
  assert(acquisition.includes('GH_TOKEN: ${{ github.token }}'));
  assert(acquisition.includes('"$RUNNER_TEMP/qualification/acquisition.json"'));
  assert(!text.slice(0, text.indexOf('    steps:')).includes('GH_TOKEN:'));
  assert(text.includes('if [ -z "$PUBLISHED_VERSION" ]; then\n            node -e'));
  assert(text.includes('      - name: Retain per-host qualification records\n        if: always()'));
});
