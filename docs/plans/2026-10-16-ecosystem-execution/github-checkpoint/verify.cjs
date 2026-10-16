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
  for (const [name, identity] of Object.entries(evidence.files)) {
    assert(!path.isAbsolute(name) && !name.split('/').includes('..'));
    const data = bytes(name);
    assert.equal(data.length, identity.length, name);
    assert.equal(sha(data), identity.sha256, name);
  }
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
  const original = report('original-native-report.json', evidence.original_source, ten, targets.keys());
  const qualified = original.records.filter(r => r.outcome !== 'reference_failure').map(key);
  const ci = report('ci-replay.json', evidence.ci_source, { equivalent: 6, assembler_rejected: 2 }, qualified);
  assert.equal(ci.reference_report_sha256, sha(bytes('original-native-report.json')));
  assert.equal(ci.reference_captured_unix_seconds, original.captured_unix_seconds);
  assert(ci.captured_unix_seconds >= ci.reference_captured_unix_seconds);
  const premerge = json('implementation-premerge.json');
  assert.equal(premerge.head, evidence.implementation_head);
  assert.equal(premerge.checks.length, 33);
  assert.equal(premerge.runs.length, 5);
  for (const item of [...premerge.checks, ...premerge.runs]) {
    assert.equal(item.status, 'completed');
    assert(['success', 'neutral', 'skipped'].includes(item.conclusion));
  }
  assert(premerge.runs.every(r => r.head_sha === premerge.head));
  assert(json('ci-source.json').merge_ref.parents.some(p => p.sha === premerge.head));
  const run = json('native-run.json');
  assert.equal(run.databaseId, 38932324538);
  assert.equal(run.headSha, native.assembler.source_commit);
  assert.equal(run.conclusion, 'failure');
  const steps = run.jobs[0].steps;
  assert.equal(steps.find(s => s.name === 'Execute the reviewed complete build plans').conclusion, 'failure');
  for (const name of ['Validate complete results before rendering', 'Render the captured table for review', 'Retain actual target evidence and proposed table']) {
    assert.equal(steps.find(s => s.name === name).conclusion, 'success');
  }
  const probe = report('isolated-native-probe.json', premerge.head, { equivalent: 1 }, ['nicodex-helloami/hello-ami']);
  const control = json('isolated-native-probe-control.json');
  assert.equal(control.positive_control.created_files['a.out'], 288);
  assert.equal(control.audit.report_sha256, sha(bytes('isolated-native-probe.json')));
  assert.equal(control.audit.ignored_sentinel_before_and_after_sha256, sha(Buffer.from(control.audit.ignored_sentinel_hex, 'hex')));
  assert(control.audit.probe_directory_removed);
  assert.match(probe.records[0].tools.vasmm68k_mot.version.cwd, /-version-probes\/vasmm68k_mot$/);
  return 'Verified ten native outcomes, eight Linux replay results, isolated native probe and exact-head CI evidence.';
}

module.exports = { verify };
if (require.main === module) console.log(verify(process.argv[2] || __dirname));
