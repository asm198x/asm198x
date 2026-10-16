'use strict';
// Verify retained outcomes, not a fresh network request or native execution.
const fs = require('node:fs');const path = require('node:path');const assert = require('node:assert/strict');const {createHash} = require('node:crypto');
const root = __dirname;const read = name => JSON.parse(fs.readFileSync(path.join(root, name)));
const files = read('files.json');assert(Object.keys(files).length > 0);
for (const [name, digest] of Object.entries(files)) assert.equal(createHash('sha256').update(fs.readFileSync(path.join(root, name))).digest('hex'), digest, name);
const targets = ['aarch64-apple-darwin', 'x86_64-apple-darwin', 'aarch64-unknown-linux-gnu', 'x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc'];
const legacyFailures = ['pasmo_word_arithmetic', 'ca65_configured_extent', 'ca65_absent_segments', 'ca65_extent_refusal'].sort();
function positive(report) {
  assert.equal(report.qualified, true);assert.equal(report.results.length, 28);
  assert.equal(new Set(report.results.map(r => r.name)).size, 28);
  assert(report.results.every(r => r.status === 'pass'));
}
for (const [folder, version, conclusion] of [['hosted-0067', '0.0.67', 'failure'], ['hosted-0069', '0.0.69', 'success']]) {
  const run = read(folder + '/run.json');const jobs = read(folder + '/jobs.json').jobs;
  assert.equal(run.head_sha, '3f36f70eba002868202ea222dd2f24a49ccf6734');assert.equal(run.conclusion, conclusion);assert.equal(jobs.length, 5);
  for (const target of targets) {
    const base = folder + '/' + target + '/';const acquisition = read(base + 'acquisition.json');const report = read(base + 'report.json');
    assert.equal(acquisition.status, 'acquired');assert.equal(acquisition.published_version, version);assert.equal(acquisition.target, target);
    assert.notEqual(acquisition.qualified, true);assert.equal(report.version, version);assert.equal(report.target, target);
    const job = jobs.find(j => j.name === `Qualify archive (${target})`);assert(job);
    assert.equal(job.steps.find(s => s.name === 'Download published archives for post-release reconciliation').conclusion, 'success');
    if (version === '0.0.67') {
      assert.equal(report.qualified, false);assert.equal(report.results.length, 28);
      assert.deepEqual(report.results.filter(r => r.status !== 'pass').map(r => r.name).sort(), legacyFailures);
    } else {
      positive(report);assert.equal(job.conclusion, 'success');
      const wrong = read(base + 'wrong-bytes.json');const corrupt = read(base + 'corrupt-archive.json');
      assert.equal(wrong.qualified, false);assert(wrong.results.some(r => r.name === 'reference_bytes' && r.status === 'fail'));
      assert.equal(corrupt.qualified, false);assert(corrupt.error.includes('SHA-256 mismatch'));
    }
  }
}
const mutation = read('hosted-0069/x86_64-pc-windows-msvc/report.json');mutation.results.pop();
assert.throws(() => positive(mutation));
console.log(`${Object.keys(files).length} file hashes and ten native acquisition/qualification outcomes verified; missing-case control rejected`);
