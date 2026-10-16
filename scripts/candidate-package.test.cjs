'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { qualifies, requiredCases, packageIdentity, targets } = require('./test-candidate-package.cjs');
const { tighten, permissive, required } = require('./generate-release-workflow.cjs');

test('qualification requires every unique expected case to pass', () => {
  const good = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(good));
  assert(!qualifies([])); assert(!qualifies(good.slice(1)));
  assert(!qualifies([...good.slice(1), good[1]]));
  assert(!qualifies([...good, { name: 'extra', status: 'pass' }]));
  for (const status of ['fail', 'unavailable', 'skipped', undefined]) {
    for (const name of requiredCases) assert(!qualifies(good.map(r => r.name === name ? { name, status } : r)));
  }
});
test('a missing target or substituted architecture in the plan cannot qualify', () => {
  const manifest = { releases: [{app_name: 'asm198x', app_version: '0.0.65', artifacts: []}], artifacts: {} };
  for (const target of targets) {
    const name = `asm198x-${target}.${target.includes('windows') ? 'zip' : 'tar.xz'}`;
    manifest.releases[0].artifacts.push(name);manifest.artifacts[name] = {target_triples: [target], checksum: name + '.sha256'};
  }
  assert.equal(packageIdentity(manifest, targets[0]).version, '0.0.65');
  for (const target of targets) {
    const broken = structuredClone(manifest);
    broken.releases[0].artifacts = broken.releases[0].artifacts.filter(n => !n.includes(target));
    assert.throws(() => packageIdentity(broken, targets[0]), /plan omits/);
  }
  const changed = structuredClone(manifest);
  changed.artifacts[changed.releases[0].artifacts[0]].target_triples = ['wrong'];
  assert.throws(() => packageIdentity(changed, targets[0]), /wrong planned target/);
});
test('generated workflow cannot publish unsuccessful or missing qualification', () => {
  const text = fs.readFileSync(path.join(__dirname, '../.github/workflows/asm198x-release.yml'), 'utf8').replaceAll('\r\n', '\n');
  const host = text.split('\n  host:\n')[1].split('\n  publish-homebrew-formula:')[0];
  assert(host.includes('      - custom-test-linux-packages'));
  assert(!host.includes(permissive));assert(host.includes(required));
  const expression = host.match(/^    if: \$\{\{ (.*) \}\}$/m)[1].replace(/needs\.([\w-]+)/g, 'needs["$1"]');
  const evaluate = new Function('needs', 'always', `return ${expression};`);
  for (const result of ['success', 'failure', 'skipped', 'cancelled', '', undefined]) {
    const needs = {plan: {result: 'success', outputs: {publishing: 'true'}}, 'build-global-artifacts': {result: 'success'}, 'build-local-artifacts': {result: 'success'}, 'custom-test-linux-packages': {result}};
    assert.equal(evaluate(needs, () => true), result === 'success', `qualification ${result}`);
  }
});
test('generator changes must be reviewed instead of silently losing the guard', () => {
  const input = '  custom-test-linux-packages:\n    needs:\n      - plan\n      - build-local-artifacts\n' + permissive;
  const result = tighten(input);
  assert(result.includes(required) && result.includes('      - build-global-artifacts'));
  assert.throws(() => tighten(input.replace(permissive, 'changed upstream guard')));
  assert.throws(() => tighten(input + permissive));
});
test('every declared release architecture has exactly one native qualification job', () => {
  const config = fs.readFileSync(path.join(__dirname, '../Cargo.toml'), 'utf8').replaceAll('\r\n', '\n');
  const declared = JSON.parse(config.match(/^targets = (\[.*\])$/m)[1]);
  const workflow = fs.readFileSync(path.join(__dirname, '../.github/workflows/test-linux-packages.yml'), 'utf8').replaceAll('\r\n', '\n');
  const matrix = [...workflow.matchAll(/^            target: ([\w-]+)$/gm)].map(m => m[1]);
  assert.deepEqual([...declared].sort(), [...targets].sort());
  assert.deepEqual(matrix.sort(), [...targets].sort());
});
test('reusable workflow writes the exact supplied candidate plan as valid JSON', () => {
  const os = require('node:os');
  const {spawnSync} = require('node:child_process');
  const workflow = fs.readFileSync(path.join(__dirname, '../.github/workflows/test-linux-packages.yml'), 'utf8').replaceAll('\r\n', '\n');
  const block = workflow.split('      - name: Save candidate plan\n')[1].split('      - name: Qualify the actual archive')[0];
  const script = block.split('        run: |\n')[1].split('\n').filter(line => line.trim()).map(line => line.slice(10)).join('\n');
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'asm198x-plan-control-'));
  try {
    fs.mkdirSync(path.join(directory, 'target'));
    const plan = {releases: [{app_name: 'asm198x', app_version: '0.0.65'}], note: 'quotes " and newlines\nkeep JSON valid'};
    const result = spawnSync('bash', ['-euo', 'pipefail', '-c', script], {cwd: directory, env: {...process.env, RELEASE_PLAN: JSON.stringify(plan)}, encoding: 'utf8', timeout: 10000});
    assert.ifError(result.error);assert.equal(result.status, 0, result.stderr);
    for (const name of ['package-test-manifest.json', 'candidate-plan.json']) assert.deepEqual(JSON.parse(fs.readFileSync(path.join(directory, 'target', name), 'utf8')), plan);
  } finally { fs.rmSync(directory, {recursive: true, force: true}); }
});
test('the older public qualification is not sufficient for this release cohort', () => {
  const old = JSON.parse(fs.readFileSync(path.join(__dirname, '../docs/plans/2026-10-15-post-fix-release-qualification/published/aarch64-apple-darwin/report.json'), 'utf8'));
  assert.equal(old.qualified, true);
  assert(!qualifies(old.results));
});

test('0.0.67 evidence cannot qualify the arithmetic and layout fixes', () => {
  const old = JSON.parse(fs.readFileSync(path.join(__dirname, '../docs/plans/2026-10-16-release-0067-checkpoint/published/aarch64-apple-darwin/report.json'), 'utf8'));
  assert.equal(old.qualified, true);
  assert.equal(qualifies(old.results), false);
  for (const name of ['pasmo_word_arithmetic', 'ca65_configured_extent', 'ca65_extent_refusal', 'ca65_absent_segments', 'ca65_used_extents']) assert(requiredCases.includes(name));
});

test('0.0.69 evidence cannot qualify the Pasmo memory-wrap correction', () => {
  const old = JSON.parse(fs.readFileSync(path.join(__dirname, '../docs/plans/2026-10-16-release-0069-checkpoint/published/aarch64-apple-darwin/report.json'), 'utf8'));
  assert.equal(old.qualified, true); assert.equal(qualifies(old.results), false);
  assert(requiredCases.includes('pasmo_wrapped_image'));
  assert(requiredCases.includes('sjasmplus_word_operators'));
  assert(requiredCases.includes('sjasmplus_logical_expressions'));
});

test('0.0.70 evidence cannot qualify the three new native parity boundaries', () => {
  const old = JSON.parse(fs.readFileSync(path.join(__dirname, '../docs/plans/2026-10-16-release-0070-checkpoint/published/aarch64-apple-darwin/report.json'), 'utf8'));
  assert.equal(old.qualified, true); assert.equal(qualifies(old.results), false);
  for (const name of ['ca65_used_extents', 'sjasmplus_logical_expressions', 'sjasmplus_shift_widths']) assert(requiredCases.includes(name));
});
