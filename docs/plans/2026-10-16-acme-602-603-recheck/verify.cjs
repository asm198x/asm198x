'use strict';
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const {createHash} = require('node:crypto');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
function verify(root) {
  const read = name => JSON.parse(fs.readFileSync(path.join(root, name)));
  const manifest = read('manifest.json');
  assert.deepEqual(Object.keys(manifest.files).sort(), ['identity.json', 'observations.json', 'probe.py', 'report.md', 'verify.cjs']);
  for (const [file, hash] of Object.entries(manifest.files)) assert.equal(sha(fs.readFileSync(path.join(root, file))), hash, file);
  const {artifact, source, receipt} = read('identity.json');
  assert.equal(artifact.version, '0.0.67'); assert.equal(artifact.target, 'aarch64-apple-darwin');
  assert.equal(source.commit, '46cedb445a01f8fefd9d2a96d63961414036fcf4');
  assert.equal(source.cargo_lock_sha256, 'a249badd5d4855f1071caf88b54ab8faaeb739fa5605de256319c534903009de');
  assert.equal(receipt.source_commit, source.commit);
  for (const key of ['committed_lock_sha256', 'before_lock_sha256', 'after_lock_sha256']) assert.equal(receipt[key], source.cargo_lock_sha256);
  for (const key of ['archive', 'target', 'version', 'archive_sha256', 'executable_sha256']) assert.equal(receipt[key], artifact[key]);
  assert.equal(receipt.signed, false); assert.equal(receipt.compiled_dependency_graph, 'unknown');
  const published = JSON.parse(fs.readFileSync(path.join(__dirname, '../2026-10-16-release-0067-checkpoint/published/aarch64-apple-darwin/report.json')));
  for (const key of ['archive_sha256', 'executable_sha256']) assert.equal(artifact[key], published[key]);
  assert.deepEqual(receipt, published.source_lock);
  const report = read('observations.json');
  assert.equal(report.qualified, true); assert.equal(report.host.machine, 'arm64');
  assert.match(report.versions[0].stdout + report.versions[0].stderr, /asm198x v0\.0\.67/);
  assert.match(report.versions[1].stdout + report.versions[1].stderr, /release 0\.97/);
  for (const v of report.versions) assert.equal(v.exit, 0);
  assert.equal(report.executables[report.versions[0].argv[0]], artifact.executable_sha256);
  for (const v of Object.values(report.executables)) assert.match(v, /^[a-f0-9]{64}$/);
  const positive = ['602_exact', '602_selected_cpu', '603_exact', '603_false_else', '603_advances', '603_logical_pc', '603_multiply_prefix'];
  const negative = ['602_code_without_origin', '603_condition_without_origin'];
  const expected = positive.flatMap(c => ['plain/human', 'plain/json', 'cbm/human', 'cbm/json', 'formatted/'].map(s => `${c}/${s}`));
  expected.push(...negative.flatMap(c => ['human', 'json'].map(mode => `${c}//${mode}`)));
  assert.equal(report.records.length, 39);
  assert.deepEqual(report.records.map(r => `${r.case}/${r.format || ''}/${r.mode || ''}`).sort(), expected.sort());
  const bytes = new Map();
  for (const r of report.records) {
    assert.equal(r.status, 'pass'); assert(r.source.trim());
    if (positive.includes(r.case)) {
      assert.equal(r.asm198x.exit, 0); assert.match(r.hex, /^(?:[a-f0-9]{2})+$/);
      if (r.format === 'formatted') { assert.equal(r.formatter.exit, 0); assert(r.formatted_source.trim()); }
      else { assert.equal(r.native.exit, 0); assert(bytes.get(`${r.case}/${r.format}`) === undefined || bytes.get(`${r.case}/${r.format}`) === r.hex); bytes.set(`${r.case}/${r.format}`, r.hex); }
    } else {
      assert.notEqual(r.native.exit, 0); assert.notEqual(r.asm198x.exit, 0);
      assert.equal(r.previous_hex, Buffer.from('previous image').toString('hex')); assert.equal(r.after_hex, r.previous_hex);
    }
    if (r.mode === 'json') { assert.equal(r.asm198x.stderr, ''); const data = JSON.parse(r.asm198x.stdout); if (positive.includes(r.case)) assert.deepEqual(data.diagnostics, []); else assert(Array.isArray(data) && data.length > 0); }
  }
  for (const c of positive) {
    assert.equal(bytes.get(`${c}/cbm`).slice(4), bytes.get(`${c}/plain`));
    assert.equal(report.records.find(r => r.case === c && r.format === 'formatted').hex, bytes.get(`${c}/plain`));
  }
  assert.equal(bytes.get('602_exact/cbm'), '0110a901'); assert.equal(bytes.get('602_selected_cpu/plain'), '6412'); assert.equal(bytes.get('603_exact/plain'), 'eaea');
  console.log('Verified published identities, 28 byte comparisons, seven formatter round trips and four rejecting controls.');
}
module.exports = {verify};
if (require.main === module) verify(path.resolve(process.argv[2] || __dirname));
