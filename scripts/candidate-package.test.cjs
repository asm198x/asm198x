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


test('0.0.71 evidence cannot qualify numeric separator support', () => {
  const old = JSON.parse(fs.readFileSync(path.join(__dirname, '../docs/plans/2026-10-16-release-0071-checkpoint/published/aarch64-apple-darwin/report.json'), 'utf8'));
  assert.equal(old.qualified, true); assert.equal(qualifies(old.results), false);
  assert(requiredCases.includes('sjasmplus_digit_separators'));
});


test('0.0.71 evidence cannot qualify SjASMPlus remainder support', () => {
  const old = JSON.parse(fs.readFileSync(path.join(__dirname, '../docs/plans/2026-10-16-release-0071-checkpoint/published/aarch64-apple-darwin/report.json'), 'utf8'));
  assert.equal(old.qualified, true); assert.equal(qualifies(old.results), false);
  assert(requiredCases.includes('sjasmplus_remainder'));
});


test('0.0.71 evidence cannot qualify DEFINE numeric substitution', () => {
  const old = JSON.parse(fs.readFileSync(path.join(__dirname, '../docs/plans/2026-10-16-release-0071-checkpoint/published/aarch64-apple-darwin/report.json'), 'utf8'));
  assert.equal(old.qualified, true); assert.equal(qualifies(old.results), false);
  assert(requiredCases.includes('sjasmplus_define_numbers'));
});


test('removing DISPLAY replay cannot satisfy current qualification', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert.equal(qualifies(complete), true);
  assert(requiredCases.includes('sjasmplus_display_format'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'sjasmplus_display_format')), false);
});


test('NEX replay cannot be omitted from current qualification', () => {
  const good = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(good)); assert(requiredCases.includes('sjasmplus_savenex'));
  assert.equal(qualifies(good.filter(r => r.name !== 'sjasmplus_savenex')), false);
});


test('forward EQU replay is required for qualification', () => {
  const good = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(good)); assert(requiredCases.includes('sjasmplus_forward_equ'));
  assert.equal(qualifies(good.filter(r => r.name !== 'sjasmplus_forward_equ')), false);
});


test('4510 replay cannot be omitted from candidate qualification', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_4510'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_4510')), false);
});

test('4510 bit branches are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_4510_bit_branches'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_4510_bit_branches')), false);
});

test('4510 PHD alias is mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_4510_phd'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_4510_phd')), false);
});

test('ca65 address-size parsing is mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_address_size'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_address_size')), false);
});


test('scoped constants cannot be omitted from package qualification', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_scoped_constants'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_scoped_constants')), false);
});


test('procedure and repeat-token native cases are required package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_scope_repetition'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_scope_repetition')), false);
});


test('include-stream ordering cannot be omitted from package qualification', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_include_streams'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_include_streams')), false);
});


test('definition-time macro capture is mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_text_capture'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_text_capture')), false);
});


test('macro introspection token boundaries are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_macro_introspection'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_macro_introspection')), false);
});


test('ca65 dotted constant refusals are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_constant_names'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_constant_names')), false);
});


test('ca65 cheap-constant anchors are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_cheap_constants'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_cheap_constants')), false);
});


test('ca65 source-time scope and width evidence is mandatory', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_scope_anchors'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_scope_anchors')), false);
});


test('ca65 CPU-specific reserved names are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_reserved_constants'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_reserved_constants')), false);
});


test('ca65 label delimiters are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_label_delimiters'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_label_delimiters')), false);
});


test('ca65 macro-definition labels are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_macro_definition_labels'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_macro_definition_labels')), false);
});


test('ca65 record allocation limits are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_record_allocators'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_record_allocators')), false);
});


test('ca65 deferred aliases are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_deferred_aliases'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_deferred_aliases')), false);
});


test('ca65 scoped bindings are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_scope_binding'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_scope_binding')), false);
});


test('ca65 unused alias validation is mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_unused_cycles'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_unused_cycles')), false);
});


test('ca65 enum expressions are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_enum_expressions'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_enum_expressions')), false);
});


test('ca65 zero-page packing is mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_zp_output'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_zp_output')), false);
});


test('ca65 BSS initialization warnings are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_bss_warnings'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_bss_warnings')), false);
});


test('ca65 scoped visibility declarations are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_scoped_exports'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_scoped_exports')), false);
});


test('ca65 label and data program counters are mandatory package evidence', () => {
  const complete = requiredCases.map(name => ({ name, status: 'pass' }));
  assert(qualifies(complete)); assert(requiredCases.includes('ca65_org_labels'));
  assert.equal(qualifies(complete.filter(r => r.name !== 'ca65_org_labels')), false);
});
