'use strict';
// Reuse the candidate gate on public assets without rebuilding them.
const fs = require('node:fs/promises');
const path = require('node:path');
const assert = require('node:assert/strict');
const { versionOf } = require('../.github/actions/setup/index.cjs');
const { packageIdentity, nativeTarget } = require('./test-candidate-package.cjs');
async function fetchPublished(version, target, directory) {
  version = versionOf(version);
  assert.equal(nativeTarget(), target);
  const base = `https://github.com/asm198x/asm198x/releases/download/asm198x-v${version}`;
  async function download(name, limit) {
    const response = await fetch(`${base}/${name}`, { signal: AbortSignal.timeout(30000) });
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
}
if (require.main === module) fetchPublished(...process.argv.slice(2)).catch(error => { console.error(error); process.exitCode = 1; });
