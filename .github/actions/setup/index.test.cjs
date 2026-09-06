'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const { createHash } = require('node:crypto');
const { versionOf, releaseAsset, verifyChecksum, runChecked, install } = require('./index.cjs');

test('version verification reads stderr and rejects unsuccessful processes', () => {
  assert.equal(runChecked(process.execPath, ['-e', "process.stderr.write('asm198x v0.0.57\\n')"], { encoding: 'utf8' }), 'asm198x v0.0.57\n');
  assert.throws(() => runChecked(process.execPath, ['-e', 'process.exit(1)'], { encoding: 'utf8' }), /failed/);
});

test('only exact safe release versions are accepted', () => {
  for (const input of ['0.0.57', 'v0.0.57', 'asm198x-v0.0.57']) assert.equal(versionOf(input), '0.0.57');
  assert.equal(versionOf('1.2.3-rc.1+build'), '1.2.3-rc.1+build');
  for (const input of ['', 'latest', 'main', '^1.2.3', '1.2', '../1.2.3', '1.2.3\nmore', '1.2.3;echo bad', '01.2.3', '1.2.3-01']) {
    assert.throws(() => versionOf(input), /version|prerelease/);
  }
});

const platforms = [
  ['darwin', 'arm64', 'aarch64-apple-darwin'],
  ['darwin', 'x64', 'x86_64-apple-darwin'],
  ['linux', 'x64', 'x86_64-unknown-linux-gnu'],
  ['win32', 'x64', 'x86_64-pc-windows-msvc'],
];

test('target selection follows the four shipped archives without emulation guesses', () => {
  for (const [platform, arch, target] of platforms) assert.equal(releaseAsset(platform, arch).target, target);
  for (const [platform, arch] of [['linux', 'arm64'], ['win32', 'arm64'], ['freebsd', 'x64']]) {
    assert.throws(() => releaseAsset(platform, arch), /no asm198x release archive/);
  }
  assert.equal(releaseAsset('win32', 'x64').member, 'asm198x.exe');
  assert.equal(releaseAsset('linux', 'x64').member, 'asm198x-x86_64-unknown-linux-gnu/asm198x');
});

test('checksum validates bytes, algorithm and the sidecar filename', () => {
  const bytes = Buffer.from('release bytes');
  const hash = createHash('sha256').update(bytes).digest('hex');
  verifyChecksum(bytes, `${hash} *archive.zip\n\n`, 'archive.zip');
  verifyChecksum(bytes, `${hash}  archive.zip\n`, 'archive.zip');
  assert.throws(() => verifyChecksum(Buffer.from('changed'), `${hash} *archive.zip`, 'archive.zip'), /mismatch/);
  assert.throws(() => verifyChecksum(bytes, `${hash} *other.zip`, 'archive.zip'), /invalid/);
  assert.throws(() => verifyChecksum(bytes, `${'0'.repeat(32)} *archive.zip`, 'archive.zip'), /invalid/);
});

async function fixture(t, platform = 'linux', arch = 'x64') {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'asm198x-setup-test-'));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  const env = { INPUT_VERSION: '0.0.57', RUNNER_TEMP: root, GITHUB_PATH: path.join(root, 'path'), GITHUB_OUTPUT: path.join(root, 'output') };
  await fs.writeFile(env.GITHUB_PATH, '');
  await fs.writeFile(env.GITHUB_OUTPUT, '');
  const bytes = Buffer.from('verified archive');
  const hash = createHash('sha256').update(bytes).digest('hex');
  const asset = releaseAsset(platform, arch);
  const fetched = [];
  const executed = [];
  return {
    env, platform, arch, fetched, executed,
    fetchImpl: async url => {
      fetched.push(url);
      return new Response(url.endsWith('.sha256') ? `${hash} *${asset.archive}\n` : bytes);
    },
    execute: (command, args) => {
      executed.push([command, args]);
      if (command === 'tar') {
        assert.equal(args[0], '-xOf');
        assert.equal(args[2], asset.member);
        return Buffer.from('executable bytes');
      }
      assert.equal(path.basename(command), asset.binary);
      assert.deepEqual(args, ['--version']);
      return 'asm198x v0.0.57\n';
    },
  };
}

for (const [platform, arch, target] of platforms) {
  test(`verified ${target} installation publishes PATH and outputs`, async t => {
    const f = await fixture(t, platform, arch);
    const result = await install(f);
    assert.equal(result.version, '0.0.57');
    assert.equal(result.target, target);
    assert.equal(await fs.readFile(f.env.GITHUB_PATH, 'utf8'), `${result.path}\n`);
    assert.equal(await fs.readFile(f.env.GITHUB_OUTPUT, 'utf8'), `version=0.0.57\ntarget=${target}\npath=${result.path}\n`);
    assert.equal(f.fetched.length, 2);
    assert.ok(f.fetched.every(url => url.startsWith('https://github.com/asm198x/asm198x/releases/download/asm198x-v0.0.57/')));
    assert.equal(await fs.readFile(path.join(result.path, releaseAsset(platform, arch).binary), 'utf8'), 'executable bytes');
  });
}

for (const failure of ['checksum', 'download', 'extract', 'version']) {
  test(`${failure} failure never publishes a binary and cleans its own staging directory`, async t => {
    const f = await fixture(t);
    if (failure === 'checksum') {
      const good = f.fetchImpl;
      f.fetchImpl = url => url.endsWith('.sha256') ? good(url) : Promise.resolve(new Response('corrupt'));
    } else if (failure === 'download') {
      f.fetchImpl = async () => new Response('missing', { status: 404 });
    } else {
      const good = f.execute;
      f.execute = (command, args) => {
        if (failure === 'extract') throw new Error('bad archive');
        return command === 'tar' ? good(command, args) : 'asm198x v0.0.56\n';
      };
    }
    await assert.rejects(install(f), /mismatch|download failed|bad archive/);
    assert.equal(await fs.readFile(f.env.GITHUB_PATH, 'utf8'), '');
    assert.equal(await fs.readFile(f.env.GITHUB_OUTPUT, 'utf8'), '');
    assert.deepEqual((await fs.readdir(f.env.RUNNER_TEMP)).sort(), ['output', 'path']);
    if (failure === 'checksum' || failure === 'download') assert.equal(f.executed.length, 0);
  });
}

test('invalid input and unsupported runners do not download or write', async t => {
  const f = await fixture(t);
  await assert.rejects(install({ ...f, arch: 'arm64' }), /no asm198x release archive/);
  await assert.rejects(install({ ...f, env: { ...f.env, INPUT_VERSION: 'latest' } }), /exact release/);
  await assert.rejects(install({ ...f, env: { ...f.env, GITHUB_PATH: '' } }), /GITHUB_PATH/);
  assert.equal(f.fetched.length, 0);
});
