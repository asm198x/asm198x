const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { verify } = require('./verify.cjs');
console.log(verify(process.argv[2] || __dirname));
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'asm198x-ecosystem-evidence-controls-'));
const source = path.resolve(process.argv[2] || __dirname);
const manifest = JSON.parse(fs.readFileSync(path.join(source, 'evidence.json')));
function reset() {
  for (const name of [...Object.keys(manifest.files), 'evidence.json']) {
    fs.copyFileSync(path.join(source, name), path.join(root, name));
  }
}
function changed(name, mutate) {
  const file = path.join(root, name);
  const data = JSON.parse(fs.readFileSync(file));
  mutate(data);
  fs.writeFileSync(file, JSON.stringify(data));
  const bytes = fs.readFileSync(file);
  const identity = JSON.parse(fs.readFileSync(path.join(root, 'evidence.json')));
  identity.files[name] = { length: bytes.length, sha256: crypto.createHash('sha256').update(bytes).digest('hex') };
  fs.writeFileSync(path.join(root, 'evidence.json'), JSON.stringify(identity));
}
const cases = [
  ['missing native report', () => fs.unlinkSync(path.join(root, 'native-report.json'))],
  ['rehashed promoted failed control', () => changed('native-report.json', r => { r.records.find(x => x.outcome === 'reference_failure').outcome = 'equivalent'; })],
  ['rehashed wrong final bytes', () => changed('native-report.json', r => { r.records[0].comparisons[0].assembler.sha256 = '0'.repeat(64); })],
  ['rehashed missing Linux shift case', () => changed('sjasmplus-shift-native.json', r => { r.cases.pop(); })],
  ['rehashed missing native step', () => changed('native-report.json', r => { r.records[0].reference_steps.pop(); })],
  ['rehashed substituted producer', () => changed('native-report.json', r => { r.assembler.source_commit = '0'.repeat(40); })],
  ['rehashed failed staging accepted', () => changed('native-run.json', r => { r.jobs[0].steps.find(s => s.name === 'Stage every required evidence file for retention').conclusion = 'failure'; })],
  ['rehashed missing process-log inventory', () => changed('artifact-inventory.json', r => { r.files = r.files.filter(f => f.path !== 'logs/build-assembler.stderr'); })],
  ['rehashed false Linux match', () => changed('sjasmplus-shift-native.json', r => { r.cases[0].hex = '00'; })],
];
try {
  for (const [name, mutate] of cases) {
    reset(); mutate();
    assert.throws(() => verify(root), undefined, name);
    console.log(`Rejected ${name}`);
  }
} finally {
  fs.rmSync(root, { recursive: true });
}
console.log('All nine frozen-evidence rejection controls passed.');
