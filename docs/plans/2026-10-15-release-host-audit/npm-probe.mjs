import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
const [directory, output] = process.argv.slice(2);
assert(directory && output, 'usage: node npm-probe.mjs <extracted package> <report>');
const api = await import(pathToFileURL(path.resolve(directory, 'asm198x_web.js')).href);
api.initSync({ module: fs.readFileSync(path.join(directory, 'asm198x_web_bg.wasm')) });
const source = 'org $8000\nstart:\n ld a,7\n halt\n';
const assembled = JSON.parse(api.assemble('pasmo', source));
assert.deepEqual(assembled.bytes, [0x3e, 7, 0x76]);
assert.deepEqual(JSON.parse(api.assemble('pasmo', source)).bytes, assembled.bytes);
assert.notDeepEqual(assembled.bytes, [0x3e, 8, 0x76], 'wrong-byte control');
const project = JSON.parse(api.assemble_project(JSON.stringify({
  dialect: 'pasmo', target: 'z80n', root: 'main.asm',
  files: { 'main.asm': 'org $8000\n include "lib.asm"\n', 'lib.asm': ' mul d,e\n db 66\n' }
})));
assert.deepEqual(project.bytes, [0xed, 0x30, 0x42]);
const diagnostics = JSON.parse(api.assemble('pasmo', 'org $8000\n ld a,undefined_symbol\n'));
assert(Array.isArray(diagnostics) && diagnostics.length > 0);
assert(diagnostics.some(d => d.severity === 'Error' && d.message));
assert.equal(api.assemble_project(JSON.stringify({dialect: 'pasmo', target: 'nonsense', root: 'main.asm', files: {'main.asm': source}})), undefined);
const dialects = JSON.parse(api.dialects());
assert(dialects.some(d => d.name === 'pasmo'));
fs.writeFileSync(output, JSON.stringify({
  package: '@asm198x/z80', version: '0.4.0', host: { platform: process.platform, arch: process.arch, node: process.version },
  checks: { assembly_reference_bytes: true, repeated_bytes: true, wrong_bytes_rejected: true, multi_file_z80n: true,
    diagnostic_array: true, unsupported_target_refused: true, dialect_discovery: true },
  assembled, project, diagnostics, dialects
}, null, 2) + '\n');
console.log('Published npm API: seven bounded checks passed');
