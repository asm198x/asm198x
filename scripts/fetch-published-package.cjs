'use strict';
// Reuse the candidate gate on public assets without rebuilding them.
const fs = require('node:fs/promises');
const path = require('node:path');
const assert = require('node:assert/strict');
const { versionOf } = require('../.github/actions/setup/index.cjs');
const { packageIdentity, nativeTarget, sha } = require('./package-identity.cjs');
async function fetchPublished(version, target, directory) {
  version = versionOf(version);
  assert.equal(nativeTarget(), target);
  const base = `https://github.com/asm198x/asm198x/releases/download/asm198x-v${version}`;
  async function download(name, limit, optional = false) {
    const response = await fetch(`${base}/${name}`, { signal: AbortSignal.timeout(30000) });
    if (optional && response.status === 404) return null;
    assert(response.ok, `download ${name}: ${response.status}`);
    const chunks = []; let size = 0;
    for await (const chunk of response.body) {
      size += chunk.length; assert(size <= limit, `download exceeds limit: ${name}`);
      chunks.push(Buffer.from(chunk));
    }
    return Buffer.concat(chunks);
  }
  const manifest = await download('dist-manifest.json', 4 * 1024 * 1024);
  const identity = packageIdentity(JSON.parse(manifest.toString()), target);
  assert.equal(identity.version, version, 'published manifest version mismatch');
  const archive = await download(identity.archive, 128 * 1024 * 1024);
  const checksum = await download(identity.archive + '.sha256', 4096);
  await fs.mkdir(path.join(directory, 'distrib'), { recursive: true });
  await fs.writeFile(path.join(directory, 'candidate-plan.json'), manifest);
  await fs.writeFile(path.join(directory, 'package-test-manifest.json'), manifest);
  await fs.writeFile(path.join(directory, 'distrib', identity.archive), archive);
  await fs.writeFile(path.join(directory, 'distrib', identity.archive + '.sha256'), checksum);
  const receiptName = `asm198x-${target}-source-lock.json`;
  const receipt = await download(receiptName, 64 * 1024, true);
  if (receipt) {
    const tag = `asm198x-v${version}`;
    const response = await fetch(`https://api.github.com/repos/asm198x/asm198x/commits/${tag}`, { signal: AbortSignal.timeout(30000) });
    assert(response.ok, `published source commit: ${response.status}`);
    const commit = (await response.json()).sha;
    assert.match(commit, /^[a-f0-9]{40}$/);
    const lock = await fetch(`https://raw.githubusercontent.com/asm198x/asm198x/${commit}/Cargo.lock`, { signal: AbortSignal.timeout(30000) });
    assert(lock.ok, `published source lock: ${lock.status}`);
    const lockBytes = Buffer.from(await lock.arrayBuffer());
    assert(lockBytes.length > 0 && lockBytes.length < 4 * 1024 * 1024);
    await fs.writeFile(path.join(directory, 'distrib', 'published-source.json'), JSON.stringify({ commit, cargo_lock_sha256: sha(lockBytes) }) + '\n');
    await fs.writeFile(path.join(directory, 'distrib', receiptName), receipt);
  }

}
if (require.main === module) fetchPublished(...process.argv.slice(2)).catch(error => { console.error(error); process.exitCode = 1; });
