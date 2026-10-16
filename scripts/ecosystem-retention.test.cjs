'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {spawnSync} = require('node:child_process');
const {test} = require('node:test');

const workflow = fs.readFileSync(path.join(__dirname, '../.github/workflows/ecosystem.yml'), 'utf8');
const block = workflow.match(/      - name: Stage every required evidence file for retention\n        if: always\(\)\n        run: \|\n([\s\S]*?)      - name: Retain actual target evidence and proposed table/);
assert(block, 'required evidence staging step');
const script = block[1].trimEnd().split('\n').map(line => line.replace(/^          /, '')).join('\n');

function fixture(fn) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'asm198x-ecosystem-retention-'));
  try {
    const cwd = path.join(root, 'work');
    const native = path.join(root, 'native');
    const files = new Map([
      ['report.json', 'native report'], ['sjasmplus-shift-native.json', 'native shifts'],
      ['logs/build-assembler.stdout', 'producer process log'], ['logs/build-assembler.stderr', 'producer stderr'],
      ['logs/assembler-version.stdout', ''], ['logs/assembler-version.stderr', 'assembler version'],
      ['ecosystem-corpus.md', 'captured table'], ['book-ecosystem.md', 'captured book table'],
    ]);
    const inputs = new Map([
      ['report.json', path.join(native, 'ecosystem-results/report.json')],
      ['sjasmplus-shift-native.json', path.join(native, 'sjasmplus-shift-native.json')],
      ...['build-assembler.stdout', 'build-assembler.stderr', 'assembler-version.stdout', 'assembler-version.stderr'].map(name => ['logs/'+name, path.join(native, 'ecosystem-results/logs', name)]),
      ['ecosystem-corpus.md', path.join(cwd, 'docs/ecosystem-corpus.md')],
      ['book-ecosystem.md', path.join(cwd, 'docs/book/src/reference/ecosystem.md')],
    ]);
    fs.mkdirSync(path.join(cwd, 'target'), {recursive: true});
    for (const [name, input] of inputs) {
      fs.mkdirSync(path.dirname(input), {recursive: true}); fs.writeFileSync(input, files.get(name));
    }
    const run = () => spawnSync('bash', ['-euo', 'pipefail', '-c', script], {
      cwd, env: {...process.env, RUNNER_TEMP: native.replaceAll('\\', '/')}, encoding: 'utf8', timeout: 10000,
    });
    fn({cwd, files, inputs, run});
  } finally { fs.rmSync(root, {recursive: true, force: true}); }
}

test('container evidence uses one relative upload root and retains every required file', () => {
  assert.match(workflow, /          path: target\/ecosystem-evidence\/\s*$/);
  fixture(({cwd, files, run}) => {
    const result = run(); assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
    for (const [name, content] of files) assert.equal(fs.readFileSync(path.join(cwd, 'target/ecosystem-evidence', name), 'utf8'), content);
    const stale = run(); assert.ifError(stale.error); assert.notEqual(stale.status, 0, 'stale staging directory accepted');
  });
});

test('missing or empty required evidence cannot produce successful staging', () => {
  for (const name of ['report.json', 'sjasmplus-shift-native.json', 'ecosystem-corpus.md', 'book-ecosystem.md']) {
    for (const mutation of ['missing', 'empty']) fixture(({inputs, run}) => {
      if (mutation === 'missing') fs.unlinkSync(inputs.get(name)); else fs.writeFileSync(inputs.get(name), '');
      const result = run(); assert.ifError(result.error); assert.notEqual(result.status, 0, `${mutation} ${name} accepted`);
    });
  }
  fixture(({inputs, run}) => {
    fs.unlinkSync(inputs.get('logs/assembler-version.stderr'));
    const result = run(); assert.ifError(result.error); assert.notEqual(result.status, 0, 'incomplete process logs accepted');
  });
  fixture(({inputs, run}) => {
    fs.rmSync(path.dirname(inputs.get('logs/build-assembler.stdout')), {recursive: true});
    const result = run(); assert.ifError(result.error); assert.notEqual(result.status, 0, 'missing process logs accepted');
  });
});
