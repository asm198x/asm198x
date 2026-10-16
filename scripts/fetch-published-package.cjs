'use strict';
// Reuse the candidate gate on public assets without rebuilding them.
const fs = require('node:fs/promises');
const path = require('node:path');
const assert = require('node:assert/strict');
const { versionOf } = require('../.github/actions/setup/index.cjs');
const { packageIdentity, nativeTarget, sha } = require('./package-identity.cjs');
async function fetchPublished(version, target, directory, fetchImpl = fetch, options = {}) {
  version = versionOf(version);
  assert.equal(nativeTarget(), target);
  const token = options.token ?? process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN;
  const acquisitionPath = options.acquisitionPath ?? path.join(directory, 'acquisition.json');
  const safe = value => {
    const text = String(value);
    return (token ? text.replaceAll(token, '[redacted]') : text).replace(/[\u0000-\u001f\u007f]/g, ' ').slice(0, 1024);
  };
  const record = {schema_version: 1, kind: 'published-release', published_version: version, target,
    checkout_commit: process.env.GITHUB_SHA || null, run: process.env.GITHUB_RUN_ID || null,
    status: 'started', stage: 'manifest'};
  async function retain() {
    await fs.mkdir(path.dirname(acquisitionPath), {recursive: true});
    await fs.writeFile(acquisitionPath, JSON.stringify(record, null, 2) + '\n');
  }
  function requireOk(response, label) {
    if (response.ok) return;
    const headers = {};
    for (const name of ['x-ratelimit-limit', 'x-ratelimit-remaining', 'x-ratelimit-reset', 'retry-after', 'x-github-request-id']) {
      const value = response.headers.get(name);
      const pattern = name === 'x-github-request-id' ? /^[A-Za-z0-9:-]{1,128}$/ : /^[0-9]{1,20}$/;
      if (value && pattern.test(value)) headers[name] = safe(value);
    }
    record.http = {status: response.status, headers};
    throw new Error(`${label}: ${response.status}`);
  }
  const base = `https://github.com/asm198x/asm198x/releases/download/asm198x-v${version}`;
  async function download(name, limit, optional = false) {
    const response = await fetchImpl(`${base}/${name}`, { signal: AbortSignal.timeout(30000) });
    if (optional && response.status === 404) return null;
    requireOk(response, `download ${name}`);
    const chunks = []; let size = 0;
    for await (const chunk of response.body) {
      size += chunk.length; assert(size <= limit, `download exceeds limit: ${name}`);
      chunks.push(Buffer.from(chunk));
    }
    return Buffer.concat(chunks);
  }
  await retain();
  try {
    const manifest = await download('dist-manifest.json', 4 * 1024 * 1024);
    const identity = packageIdentity(JSON.parse(manifest.toString()), target);
    assert.equal(identity.version, version, 'published manifest version mismatch');
    record.stage = 'archive';
    const archive = await download(identity.archive, 128 * 1024 * 1024);
    record.stage = 'checksum';
    const checksum = await download(identity.archive + '.sha256', 4096);
    await fs.mkdir(path.join(directory, 'distrib'), { recursive: true });
    await fs.writeFile(path.join(directory, 'candidate-plan.json'), manifest);
    await fs.writeFile(path.join(directory, 'package-test-manifest.json'), manifest);
    await fs.writeFile(path.join(directory, 'distrib', identity.archive), archive);
    await fs.writeFile(path.join(directory, 'distrib', identity.archive + '.sha256'), checksum);
    const receiptName = `asm198x-${target}-source-lock.json`;
    record.stage = 'receipt';
    const receipt = await download(receiptName, 64 * 1024, true);
    record.receipt = receipt ? 'present' : 'absent';
    if (receipt) {
      const tag = `asm198x-v${version}`;
      record.stage = 'source_commit';
      // The existing read-only token goes only to this fixed API host. Do not
      // send it to public asset/raw hosts or follow an authenticated redirect.
      const headers = {Accept: 'application/json'};
      if (token) headers.Authorization = `Bearer ${token}`;
      const response = await fetchImpl(`https://api.github.com/repos/asm198x/asm198x/commits/${tag}`, {
        signal: AbortSignal.timeout(30000), headers, redirect: 'error',
      });
      requireOk(response, 'published source commit');
      const commit = (await response.json()).sha;
      assert.match(commit, /^[a-f0-9]{40}$/);
      record.stage = 'source_lock';
      const lock = await fetchImpl(`https://raw.githubusercontent.com/asm198x/asm198x/${commit}/Cargo.lock`, { signal: AbortSignal.timeout(30000) });
      requireOk(lock, 'published source lock');
      const lockBytes = Buffer.from(await lock.arrayBuffer());
      assert(lockBytes.length > 0 && lockBytes.length < 4 * 1024 * 1024);
      await fs.writeFile(path.join(directory, 'distrib', 'published-source.json'), JSON.stringify({ commit, cargo_lock_sha256: sha(lockBytes) }) + '\n');
      await fs.writeFile(path.join(directory, 'distrib', receiptName), receipt);
    }
    record.status = 'acquired';record.stage = 'complete';
    await retain();
  } catch (error) {
    record.status = 'failed';record.qualified = false;record.error = safe(error.message);
    await retain();
    throw new Error(record.error);
  }
}
module.exports = { fetchPublished };
if (require.main === module) {
  const [version, target, directory, acquisitionPath] = process.argv.slice(2);
  fetchPublished(version, target, directory, fetch, {acquisitionPath}).catch(error => { console.error(error); process.exitCode = 1; });
}
