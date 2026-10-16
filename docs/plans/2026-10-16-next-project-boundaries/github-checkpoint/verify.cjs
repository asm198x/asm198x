// Frozen evidence checks; deliberately independent of the evolving xtask.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const ok = p => p && p.exit === 0 && p.failure === null;
const validHash = h => /^[a-f0-9]{64}$/.test(h);

function verify(root = __dirname) {
  const bytes = name => fs.readFileSync(path.join(root, name));
  const json = name => JSON.parse(bytes(name));
  const evidence = json('evidence.json');
  assert.deepEqual(Object.keys(evidence.files).sort(), ['README.md', 'corpus.json', 'cc65-opcodes.cfg', 'native-report.json', 'captured-native-report.json', 'native-run.json', 'native-job.log', 'sjasmplus-shift-widths.json', 'sjasmplus-shift-native.json', 'ecosystem-corpus.md', 'book-ecosystem.md', 'verify.cjs', 'verify-controls.cjs', 'capture-artifact.zip', 'artifact-api.json', 'artifact-inventory.json', 'captured-ecosystem-corpus.md', 'captured-book-ecosystem.md'].sort());
  for (const [name, identity] of Object.entries(evidence.files)) {
    assert(!path.isAbsolute(name) && !name.split('/').includes('..'));
    const data = bytes(name);
    assert.equal(data.length, identity.length, name);
    assert.equal(sha(data), identity.sha256, name);
  }
  const api = json('artifact-api.json');
  assert.equal(api.workflow_run.id, evidence.native_run);
  assert.equal(api.workflow_run.head_sha, evidence.native_source);
  assert.equal(api.workflow_run.head_branch, 'main');
  assert.equal(api.name, 'ecosystem-build-results'); assert.equal(api.expired, false);
  assert.equal(api.size_in_bytes, bytes('capture-artifact.zip').length);
  assert.equal(api.digest, 'sha256:'+sha(bytes('capture-artifact.zip')));
  const inventory = json('artifact-inventory.json');
  assert.equal(inventory.run, evidence.native_run);
  const entries = new Map(inventory.files.map(f => [f.path, f]));
  assert.equal(entries.size, inventory.files.length);
  for (const [artifact, retained] of [['report.json','captured-native-report.json'], ['sjasmplus-shift-native.json','sjasmplus-shift-native.json'], ['ecosystem-corpus.md','captured-ecosystem-corpus.md'], ['book-ecosystem.md','captured-book-ecosystem.md']]) {
    const item=entries.get(artifact); assert(item);
    assert.equal(item.length, bytes(retained).length); assert.equal(item.sha256, sha(bytes(retained)));
  }
  for (const name of ['build-assembler.stdout','build-assembler.stderr','assembler-version.stdout','assembler-version.stderr']) assert(entries.has('logs/'+name));
  assert(inventory.files.filter(f => f.path.startsWith('logs/')).length > 4);
  const manifest = json('corpus.json');
  const targets = new Map(manifest.projects.flatMap(p =>
    p.targets.map(t => [`${p.id}/${t.id}`, { project: p, target: t }])));
  const key = r => `${r.project}/${r.target}`;
  const counts = records => records.reduce((out, r) => {
    out[r.outcome] = (out[r.outcome] || 0) + 1;
    return out;
  }, {});
  const artifact = a => {
    assert(a.length > 0 && validHash(a.sha256));
    if (a.hex !== null && a.hex !== undefined) {
      assert(/^(?:[a-f0-9]{2})*$/.test(a.hex));
      const data = Buffer.from(a.hex, 'hex');
      assert.equal(data.length, a.length);
      assert.equal(sha(data), a.sha256);
    }
  };
  const last = (steps, name) => steps.slice().reverse()
    .flatMap(s => s.outputs).find(a => a.path === name);
  function report(name, producer, expectedCounts, expectedKeys) {
    expectedKeys = [...expectedKeys];
    const r = json(name);
    assert.equal(r.schema, 1);
    assert.equal(r.manifest_sha256, sha(bytes('corpus.json')));
    assert.equal(r.assembler.source_commit, producer);
    assert(validHash(r.assembler.executable_sha256));
    assert(validHash(r.assembler.cargo_lock_sha256));
    assert(ok(r.assembler.build) && ok(r.assembler.version));
    assert.deepEqual(counts(r.records), expectedCounts);
    assert.deepEqual(r.records.map(key).sort(), [...expectedKeys].sort());
    assert.deepEqual([...r.selected].sort(), [...expectedKeys].sort());
    assert.equal(new Set(r.records.map(key)).size, r.records.length);
    for (const row of r.records) {
      const { project, target } = targets.get(key(row));
      const plan = target.build;
      assert.equal(row.source_commit, project.commit);
      for (const step of [...row.reference_steps, ...row.assembler_steps]) {
        for (const output of step.outputs) artifact(output);
      }
      if (row.outcome === 'reference_failure') {
        assert(row.reference_steps.some(s => !ok(s.process)));
        assert.equal(row.assembler_steps.length, 0);
        assert.match(row.first_failure, /ca65: Invalid CPU: '(6502dtv|65ce02)'/);
        continue;
      }
      assert.equal(row.reference_steps.length, plan.reference.length);
      assert(row.reference_steps.every(s => ok(s.process)));
      assert.equal(row.license.sha256, sha(Buffer.from(row.license.text)));
      assert.equal(row.license.spdx, project.license.spdx);
      for (const tool of plan.tools) {
        const identity = row.tools[tool.name];
        assert(validHash(identity.sha256) && ok(identity.version));
        const lines = (identity.version.stdout + identity.version.stderr)
          .split(/\r?\n/).map(s => s.trim());
        assert(tool.versions.some(v => lines.includes(v)));
      }
      for (const adapter of row.adapters) {
        assert.equal(adapter.path, 'ecosystem/adapters/cc65-opcodes.cfg');
        assert.equal(adapter.sha256, sha(bytes('cc65-opcodes.cfg')));
      }
      for (const pair of plan.compare) artifact(last(row.reference_steps, pair.reference));
      if (row.outcome === 'assembler_rejected') {
        assert(row.assembler_steps.some(s => !ok(s.process)));
        assert(row.first_failure);
        continue;
      }
      assert.equal(row.outcome, 'equivalent');
      assert.equal(row.first_failure, null);
      assert.equal(row.assembler_steps.length, plan.assembler.length);
      assert(row.assembler_steps.every(s => ok(s.process)));
      assert.equal(row.comparisons.length, plan.compare.length);
      for (let i = 0; i < plan.compare.length; i++) {
        const pair = row.comparisons[i];
        assert.equal(pair.reference.path, plan.compare[i].reference);
        assert.equal(pair.assembler.path, plan.compare[i].assembler);
        artifact(pair.reference); artifact(pair.assembler);
        assert(pair.equal);
        assert.equal(pair.reference.length, pair.assembler.length);
        assert.equal(pair.reference.sha256, pair.assembler.sha256);
        for (const [steps, result] of [[row.reference_steps, pair.reference], [row.assembler_steps, pair.assembler]]) {
          const actual = last(steps, result.path);
          assert.equal(actual.length, result.length);
          assert.equal(actual.sha256, result.sha256);
        }
      }
    }
    return r;
  }
  const ten = { equivalent: 6, reference_failure: 2, assembler_rejected: 2 };
  const native = report('native-report.json', evidence.native_source, ten, targets.keys());
  const captured = json('captured-native-report.json');
  const withoutIssues = data => { const copy=structuredClone(data); for (const row of copy.records) delete row.issues; return copy; };
  assert.deepEqual(withoutIssues(native), withoutIssues(captured), 'triage changed native evidence');
  for (const row of native.records.filter(r => r.outcome === 'reference_failure')) {
    assert.deepEqual(row.issues, ['https://github.com/asm198x/asm198x/issues/718']);
  }
  const next = native.records.filter(r => r.outcome === 'assembler_rejected');
  const form = next.find(r => r.project === 'cc65-4510-opcodes');
  const separators = next.find(r => r.project === 'howprice-specnext-invaders');
  assert(form && separators);
  assert.match(form.first_failure, /no \(indirect\),z form/);
  assert.match(separators.first_failure, /malformed character literal/);
  assert.deepEqual(form.issues, ['https://github.com/asm198x/asm198x/issues/722']);
  assert.deepEqual(separators.issues, ['https://github.com/asm198x/asm198x/issues/725']);
  const run = json('native-run.json');
  assert.equal(run.id, evidence.native_run);
  assert.equal(run.head_sha, evidence.native_source);
  assert.equal(run.status, 'completed'); assert.equal(run.conclusion, 'failure');
  assert.equal(run.jobs.length, 1);
  const steps = run.jobs[0].steps;
  assert.equal(steps.find(s => s.name === 'Execute the reviewed complete build plans').conclusion, 'failure');
  for (const name of ['Corroborate native SjASMPlus shift widths on Linux', 'Stage every required evidence file for retention', 'Validate complete results before rendering', 'Render the captured table for review', 'Retain actual target evidence and proposed table']) {
    assert.equal(steps.find(s => s.name === name).conclusion, 'success');
  }
  const facts = json('sjasmplus-shift-widths.json');
  const live = json('sjasmplus-shift-native.json');
  assert.equal(live.os, 'linux'); assert.equal(live.arch, 'x86_64');
  assert(live.version.includes('v1.21.0')); assert(validHash(live.sha256));
  assert.equal(facts.cases.length, 646);
  assert.equal(facts.cases.filter(c => c.exit === 0).length, 636);
  assert.deepEqual(live.cases.map(c => c.name), facts.cases.map(c => c.name));
  for (let i=0; i<facts.cases.length; i++) {
    const actual = live.cases[i], expected = facts.cases[i];
    assert(actual.matches);
    if (expected.exit === 0) { assert.equal(actual.exit, 0); assert.equal(actual.hex, expected.hex); }
    else { assert(Number.isInteger(actual.exit) && actual.exit !== 0); }
  }
  return 'Verified ten fresh native outcomes, the next project boundaries and 646 live Linux shift controls.';
}
module.exports = {verify};
if (require.main === module) console.log(verify(process.argv[2] || __dirname));
