'use strict';
// cargo-dist 0.32 permits skipped custom jobs to reach hosting. Keep its full
// generated workflow, then tighten only our required qualification dependency.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const workflow = '.github/workflows/asm198x-release.yml';
const permissive = "(needs.custom-test-linux-packages.result == 'skipped' || needs.custom-test-linux-packages.result == 'success')";
const required = "(needs.custom-test-linux-packages.result == 'success')";
function tighten(text) {
  assert.equal(text.split(permissive).length, 2, 'cargo-dist qualification guard changed; review generation');
  const dependency = '  custom-test-linux-packages:\n    needs:\n      - plan\n      - build-local-artifacts\n';
  assert.equal(text.split(dependency).length, 2, 'cargo-dist qualification dependencies changed');
  return text.replace(permissive, required).replace(dependency, dependency + '      - build-global-artifacts\n');
}
function run(command, args, cwd) {
  const p = spawnSync(command, args, { cwd, encoding: 'utf8', timeout: 120000, maxBuffer: 4 * 1024 * 1024 });
  if (p.error) throw p.error;
  assert.equal(p.status, 0, `${command}: ${p.stderr}`);
  return p.stdout;
}
function generate(root, check) {
  // Generate in a disposable copy so --check never changes the user's checkout.
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'asm198x-release-generation-'));
  try {
    const files = run('git', ['-c', 'core.fsmonitor=false', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], root).split('\0').filter(Boolean);
    for (const file of new Set(files)) {
      if (!fs.lstatSync(path.join(root, file)).isFile()) continue;
      const destination = path.join(temporary, file);
      fs.mkdirSync(path.dirname(destination), { recursive: true });
      fs.copyFileSync(path.join(root, file), destination);
    }
    // dist refuses generation while allow-dirty=ci is enabled. Remove that
    // exception only in the disposable copy; the full output is still checked.
    const cargo = path.join(temporary, 'Cargo.toml');
    const config = fs.readFileSync(cargo, 'utf8');
    assert.equal(config.split('allow-dirty = ["ci"]').length, 2);
    fs.writeFileSync(cargo, config.replace('allow-dirty = ["ci"]', 'allow-dirty = []'));
    run('dist', ['generate', '--mode', 'ci'], temporary);
    const result = tighten(fs.readFileSync(path.join(temporary, workflow), 'utf8').replaceAll('\r\n', '\n'));
    if (check) assert.equal(fs.readFileSync(path.join(root, workflow), 'utf8').replaceAll('\r\n', '\n'), result, 'Release workflow is stale; run node scripts/generate-release-workflow.cjs');
    else fs.writeFileSync(path.join(root, workflow), result);
  } finally { fs.rmSync(temporary, { recursive: true, force: true }); }
}
module.exports = { tighten, generate, permissive, required };
if (require.main === module) generate(path.resolve(__dirname, '..'), process.argv.includes('--check'));
