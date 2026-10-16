'use strict';
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const assert = require('node:assert/strict');
const {createHash} = require('node:crypto');
const {verify} = require('./verify.cjs');
const source = path.resolve(process.argv[2] || __dirname);
const target = 'aarch64-apple-darwin';
verify(source);
function control(name, change, reason) {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'asm198x-evidence-control-'));
  try {
    fs.cpSync(source, temp, {recursive: true});
    const manifest = JSON.parse(fs.readFileSync(path.join(temp, 'manifest.json')));
    const mutate = (relative, fn) => {
      const file = path.join(temp, relative);
      const data = JSON.parse(fs.readFileSync(file)); fn(data);
      fs.writeFileSync(file, JSON.stringify(data, null, 2) + '\n');
      manifest.files[relative] = createHash('sha256').update(fs.readFileSync(file)).digest('hex');
    };
    change(temp, mutate, manifest);
    fs.writeFileSync(path.join(temp, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
    assert.throws(() => verify(temp), reason, `${name} was accepted`);
    console.log(`Rejected ${name}`);
  } finally { fs.rmSync(temp, {recursive: true, force: true}); }
}
control('missing report', temp => fs.unlinkSync(path.join(temp, `published/${target}/report.json`)), /ENOENT/);
control('rehashed unavailable case', (_, mutate) => mutate(`published/${target}/report.json`, r => { r.results[0].status = 'unavailable'; }), /qualification/);
control('rehashed missing producer receipt', (_, mutate) => mutate(`published/${target}/report.json`, r => { delete r.source_lock; }), /missing producer receipt/);
control('rehashed changed lock', (_, mutate) => mutate(`published/${target}/report.json`, r => { r.source_lock.after_lock_sha256 = '0'.repeat(64); }), /Expected values to be strictly equal/);
control('rehashed substituted public executable', (_, mutate) => {
  for (const file of ['report.json', 'wrong-bytes.json']) mutate(`published/${target}/${file}`, r => { r.executable_sha256 = '0'.repeat(64); r.source_lock.executable_sha256 = r.executable_sha256; });
}, /Expected values to be strictly equal|published receipt|publication identity/);
control('missing manifest entry', (_, __, manifest) => { delete manifest.files[`published/${target}/report.json`]; }, /Expected values to be strictly deep-equal/);
console.log('All six retained-evidence rejection controls passed.');
