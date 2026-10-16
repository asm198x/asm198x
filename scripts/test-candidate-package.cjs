'use strict';
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { verifyChecksum, runChecked } = require('../.github/actions/setup/index.cjs');
const { targets, sha, packageIdentity, nativeTarget, binaryTarget, archiveIdentity } = require('./package-identity.cjs');
const { verifyReceipt, sourceIdentity } = require('./build-locked-package.cjs');
const requiredCases = ['archive_identity', 'version', 'reference_bytes', 'determinism', 'structured_success', 'structured_failure', 'preserve_previous_outputs', 'missing_input_json', 'unknown_cpu', 'incompatible_cpu', 'hardlink_source', 'hardlink_sidecar', 'formatter_container', 'formatter_origin', 'lua', 'upstream_project'];
requiredCases.push('disasm_options', 'disasm_wide_origin', 'pasmo_nested_repeat', 'quoted_data_native', 'quoted_data_format', 'conversion_quoted_data', 'conversion_file_scope', 'sjasmplus_decoded_escapes');
requiredCases.push('pasmo_word_arithmetic', 'ca65_configured_extent', 'ca65_extent_refusal', 'ca65_absent_segments', 'pasmo_wrapped_image');
requiredCases.push('sjasmplus_word_operators');
const sourceFiles = {
  'LICENSE': '62cd567bc9270c722abcaa6ad88b6a1fe861698d63890f787cc557d2395be203',
  'helloworld.asm': '9e57c82435c0cf35e56089d56d6b83991c61624c2b3448ab6c66fb13a884a0a5',
  'libs/loader.asm': '32b0dfd80eacbbdfa69774280122d9c7f115fe0602a14bc5d630b24e08cd82cc',
  'libs/screen.asm': 'd33fbb4f0d549567438138efa38adbb4eb154aa17d6e1c0dee496f065cf469c6',
};
const upstreamCommit = '35b4968be3bdd26804a5d195557d1225497a0358';
function qualifies(results) {
  return results.length === requiredCases.length && new Set(results.map(r => r.name)).size === requiredCases.length &&
    requiredCases.every(name => results.some(r => r.name === name && r.status === 'pass'));
}
async function audit(plan, target, dist, { corruptOutput = false, corruptArchive = false, requireSourceLock = false, source = null } = {}) {
  const identity = packageIdentity(plan, target);
  assert.equal(nativeTarget(), target, 'candidate must execute on its native runner');
  const archivePath = path.join(dist, identity.archive);
  let archive = fs.readFileSync(archivePath);
  if (corruptArchive) archive = Buffer.concat([archive, Buffer.from('corrupt')]);
  const checksum = fs.readFileSync(archivePath + '.sha256', 'utf8');
  verifyChecksum(archive, checksum, identity.archive);
  assert.throws(() => verifyChecksum(Buffer.concat([archive, Buffer.from('corrupt')]), checksum, identity.archive), /SHA-256 mismatch/);
  const artifact = archiveIdentity(plan, target, dist);
  const executable = artifact.executable;
  const receiptPath = path.join(dist, `asm198x-${target}-source-lock.json`);
  let sourceLock = null;
  if (requireSourceLock || fs.existsSync(receiptPath)) {
    const receipt = JSON.parse(fs.readFileSync(receiptPath, 'utf8'));
    const expected = source || sourceIdentity(path.resolve(__dirname, '..'));
    verifyReceipt(receipt, artifact, expected, source ? null : process.env);
    sourceLock = receipt;
  }
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'asm198x-candidate-'));
  const results = [];
  const program = path.join(root, target.includes('windows') ? 'asm198x.exe' : 'asm198x');
  fs.writeFileSync(program, executable, { mode: 0o755 });
  let runs;
  function run(args, cwd = root) {
    const p = spawnSync(program, args, { cwd, encoding: 'utf8', timeout: 30000, maxBuffer: 4 * 1024 * 1024 });
    runs.push({ argv: [program, ...args], cwd, exit: p.status, signal: p.signal, stdout: p.stdout, stderr: p.stderr, error: p.error?.message });
    if (p.error) throw p.error;
    assert.equal(p.signal, null, 'candidate terminated by signal');
    return p;
  }
  const read = name => fs.readFileSync(path.join(root, name));
  const write = (name, data) => fs.writeFileSync(path.join(root, name), data);
  const test = (name, fn) => {
    runs = [];
    try { fn(); results.push({ name, status: 'pass', runs }); }
    catch (error) { results.push({ name, status: 'fail', reason: error.message, runs }); }
  };
  const reject = p => assert(p.status !== 0 && p.status !== null, 'invalid selection/input was accepted');
  try {
    write('good.s', '*=$8000\nstart:\n lda #$05\n rts\n');
    write('bad.s', '*=$8000\n lda #$fff\n');
    write('z80.s', 'org $8000\nnop\n');
    test('archive_identity', () => { assert.equal(binaryTarget(executable), target); });
    test('version', () => {
      const p = run(['--version']); assert.equal(p.status, 0);
      assert.equal((p.stdout + p.stderr).trim(), `asm198x v${identity.version}`);
    });
    test('reference_bytes', () => {
      assert.equal(run(['--dialect', 'acme', 'good.s', '-o', 'first.bin']).status, 0);
      if (corruptOutput) write('first.bin', 'wrong bytes');
      assert.deepEqual(read('first.bin'), Buffer.from('a90560', 'hex'));
    });
    test('determinism', () => {
      assert.equal(run(['--dialect', 'acme', 'good.s', '-o', 'second.bin']).status, 0);
      assert.deepEqual(read('first.bin'), read('second.bin'));
    });
    test('structured_success', () => {
      const p = run(['--dialect', 'acme', '--message-format=json', 'good.s', '-o', 'json.bin']);
      assert.equal(p.status, 0); assert.equal(p.stderr, '');
      const obj = JSON.parse(p.stdout); assert.deepEqual(obj.bytes, [...read('first.bin')]);
      assert.equal(obj.origin, 32768); assert.equal(obj.symbols.start, 32768); assert.deepEqual(obj.diagnostics, []);
      assert.deepEqual(read('json.bin'), Buffer.from(obj.bytes));
    });
    test('structured_failure', () => {
      const human = run(['--dialect', 'acme', 'bad.s', '-o', 'bad.bin']); reject(human);
      const p = run(['--dialect', 'acme', '--message-format=json', 'bad.s', '-o', 'bad.bin']); reject(p);
      assert.equal(p.stderr, ''); const diagnostics = JSON.parse(p.stdout);
      assert(Array.isArray(diagnostics) && diagnostics.length > 0);
      assert(diagnostics.some(d => d.severity === 'Error' && d.message && d.span?.line === 2 && human.stderr.includes(d.message)));
      assert(!fs.existsSync(path.join(root, 'bad.bin')));
    });
    test('preserve_previous_outputs', () => {
      const names = ['previous.bin', 'previous.debug198x', 'previous.sym', 'previous.lst'];
      for (const name of names) write(name, 'preserve previous');
      const p = run(['--dialect', 'acme', 'bad.s', '-o', names[0], '--debug=' + names[1], '--sym=' + names[2], '--listing=' + names[3]]); reject(p);
      for (const name of names) assert.equal(read(name).toString(), 'preserve previous');
    });
    test('missing_input_json', () => {
      const p = run(['--message-format=json', 'missing.s', '-o', 'missing.bin']); reject(p);
      const diagnostics = JSON.parse(p.stdout);
      assert(Array.isArray(diagnostics) && diagnostics.some(d => d.severity === 'Error' && d.message)); assert.equal(p.stderr, '');
      assert(!fs.existsSync(path.join(root, 'missing.bin')));
    });
    for (const [name, cpu] of [['unknown_cpu', 'nonsense'], ['incompatible_cpu', '68000']]) test(name, () => {
      const p = run(['--dialect', 'pasmo', '--cpu', cpu, '--message-format=json', 'z80.s', '-o', name + '.bin']); reject(p);
      const diagnostics = JSON.parse(p.stdout); assert.equal(p.stderr, '');
      assert(diagnostics.some(d => d.message.includes(cpu) && (cpu === 'nonsense' || d.message.includes('pasmo'))));
      assert(!fs.existsSync(path.join(root, name + '.bin')));
    });
    test('hardlink_source', () => {
      write('linked.s', '*=$8000\n nop\n'); fs.linkSync(path.join(root, 'linked.s'), path.join(root, 'linked.bin'));
      reject(run(['linked.s', '-o', 'linked.bin'])); assert.equal(read('linked.s').toString(), '*=$8000\n nop\n');
    });
    test('hardlink_sidecar', () => {
      write('sidecar.s', '*=$8000\n nop\n'); fs.linkSync(path.join(root, 'sidecar.s'), path.join(root, 'sidecar.sym'));
      reject(run(['sidecar.s', '-o', 'sidecar.bin', '--sym=sidecar.sym']));
      assert.equal(read('sidecar.s').toString(), '*=$8000\n nop\n');
    });
    for (const [name, flags] of [['formatter_container', ['--prg']], ['formatter_origin', ['--org', '0']]]) test(name, () => {
      reject(run(['fmt', ...flags, 'good.s', '-o', name + '.s']));
      assert(!fs.existsSync(path.join(root, name + '.s')));
    });
    test('lua', () => {
      write('lua.s', ' LUA ALLPASS\n sj.add_byte(7)\n ENDLUA\n');
      assert.equal(run(['--dialect', 'sjasmplus', 'lua.s', '-o', 'lua.bin']).status, 0); assert.deepEqual(read('lua.bin'), Buffer.from([7]));
    });
    const nativeCase = (file, name, dialect) => {
      const data = JSON.parse(fs.readFileSync(path.join(__dirname, '../crates/asm198x/tests/fixtures', file + '.json'), 'utf8'));
      const fact = data.cases.find(c => (c.name === name || c.escape === name) && (!dialect || c.dialect === dialect));
      assert(fact && fact.exit === 0 && typeof fact.source === 'string' && fact.source.trim());
      assert.match(fact.hex, /^(?:[a-f0-9]{2})+$/i);
      return fact;
    };
    const quoted = [
      ['pasmo', [], nativeCase('pasmo-quoted-semicolon', 'mixed')],
      ['pasmonext', [], nativeCase('pasmo-character-data-lists', 'quote', 'pasmo')],
      ['pasmo', [], nativeCase('pasmo-character-data-lists', 'comma', 'pasmo')],
      ['sjasmplus', [], nativeCase('sjasmplus-quoted-comments', 'mixed')],
      ['sjasmplus', ['--cpu', 'z80n'], nativeCase('sjasmplus-quoted-comments', 'comment_order')],
      ['sjasmplus', [], nativeCase('sjasmplus-quoted-comments', 'backslash_character')],
      ['sjasmplus', [], nativeCase('sjasmplus-escape-boundaries', '\\q')],
    ];
    test('disasm_options', () => {
      write('raw-z80.bin', Buffer.from([0]));
      const ordinary = run(['disasm', '--cpu', 'z80', 'raw-z80.bin']);
      assert.equal(ordinary.status, 0); assert.match(ordinary.stdout, /nop/i);
      write('listing.s', 'previous source');
      for (const flags of [['-o', 'listing.s'], ['--map=listing.s']]) {
        const p = run(['disasm', '--cpu', 'z80', 'raw-z80.bin', ...flags]); reject(p);
        assert.equal(p.stdout, ''); assert.match(p.stderr, /disasm/);
        assert.equal(read('listing.s').toString(), 'previous source');
      }
    });
    test('disasm_wide_origin', () => {
      write('m68k.bin', Buffer.from('6002', 'hex'));
      const p = run(['disasm', '--cpu', '68000', '--org', '0xff8000', 'm68k.bin']);
      assert.equal(p.status, 0); assert.match(p.stdout, /bra\.s\s+\$ff8004/i);
      assert.match(p.stderr, /ff8000/i); assert.deepEqual(read('m68k.bin'), Buffer.from('6002', 'hex'));
    });
    test('pasmo_nested_repeat', () => {
      const fact = nativeCase('pasmo-nested-repetition', 'two_levels');
      write('nested.s', fact.source);
      for (const dialect of ['pasmo', 'pasmonext']) {
        assert.equal(run(['--dialect', dialect, 'nested.s', '-o', 'nested.bin']).status, 0);
        assert.deepEqual(read('nested.bin'), Buffer.from(fact.hex, 'hex'));
      }
      assert.equal(read('nested.s').toString(), fact.source);
    });
    test('pasmo_wrapped_image', () => {
      const fact = nativeCase('pasmo-output-wrap', 'subsequent_pc');
      write('wrap.s', fact.source);
      for (const dialect of ['pasmo', 'pasmonext']) {
        const p = run(['--dialect', dialect, '--message-format=json', 'wrap.s', '-o', 'wrap.bin']);
        assert.equal(p.status, 0); assert.equal(p.stderr, '');
        assert.deepEqual(read('wrap.bin'), Buffer.from(fact.hex, 'hex'));
        const result = JSON.parse(p.stdout); assert.equal(result.origin, 0);
        assert.equal(result.symbols.first, 65535); assert.equal(result.symbols.after, 1);
        assert.equal(result.start, 1); assert(result.warnings.some(w => w.message.includes('64KB')));
        write('wrap.s', ' org $ffff\n jr target\ntarget: db 1\n');
        write('wrap.bin', 'previous image'); reject(run(['--dialect', dialect, 'wrap.s', '-o', 'wrap.bin']));
        assert.equal(read('wrap.bin').toString(), 'previous image');
        write('wrap.s', fact.source);
      }
    });
    test('sjasmplus_word_operators', () => {
      for (const name of ['equ', 'macro_repeat', 'expr_25']) {
        const fact = nativeCase('sjasmplus-word-operators', name);
        for (const cpu of ['z80', 'z80n']) {
          write('words.s', fact.source);
          const p = run(['--dialect', 'sjasmplus', '--cpu', cpu, '--message-format=json', 'words.s', '-o', 'words.bin']);
          assert.equal(p.status, 0); assert.equal(p.stderr, '');
          assert.deepEqual(read('words.bin'), Buffer.from(fact.hex, 'hex'));
          write('words.s', ' dw 1 xOr 2\n');
          reject(run(['--dialect', 'sjasmplus', '--cpu', cpu, 'words.s', '-o', 'words.bin']));
          assert.deepEqual(read('words.bin'), Buffer.from(fact.hex, 'hex'));
        }
      }
    });
    test('pasmo_word_arithmetic', () => {
      for (const name of ['dw_0', 'dw_1', 'dw_3', 'dw_15', 'dw_28', 'dw_29', 'dw_37']) {
        const fact = nativeCase('pasmo-arithmetic', name);
        write('arithmetic.s', fact.source);
        for (const dialect of ['pasmo', 'pasmonext']) {
          const p = run(['--dialect', dialect, 'arithmetic.s', '-o', 'arithmetic.bin']);
          assert.equal(p.status, 0); assert.deepEqual(read('arithmetic.bin'), Buffer.from(fact.hex, 'hex'));
        }
      }
    });
    test('ca65_configured_extent', () => {
      for (const name of ['CODE', 'PROGRAM']) {
        write('extent.cfg', `MEMORY { PRG: start=$8000, size=$8000, file=%O, fill=yes; } SEGMENTS { ${name}: load=PRG, type=ro; }`);
        write('extent.s', `.segment "${name}"\n.res $7ffa,$ea\n.word $8000,$8001,$8002\n`);
        const p = run(['--dialect', 'ca65', '-C', 'extent.cfg', 'extent.s', '-o', 'extent.bin']);
        assert.equal(p.status, 0);
        const expected = Buffer.concat([Buffer.alloc(0x7ffa, 0xea), Buffer.from('008001800280', 'hex')]);
        assert.deepEqual(read('extent.bin'), expected);
      }
    });
    test('ca65_absent_segments', () => {
      const pinned = 'MEMORY { PRG: start=$8000, size=$8000, file=%O, fill=yes; } SEGMENTS { PROGRAM: load=PRG, type=ro; TABLE: load=PRG, type=ro, start=$fffa; }';
      write('absent.cfg', pinned); write('absent.s', '.segment "PROGRAM"\n.res $8000,$ea\n');
      const args = ['--dialect', 'ca65', '-C', 'absent.cfg', 'absent.s', '-o', 'absent.bin'];
      assert.equal(run(args).status, 0); assert.deepEqual(read('absent.bin'), Buffer.alloc(0x8000, 0xea));
      write('absent.s', '.segment "PROGRAM"\n.res $8000,$ea\n.segment "TABLE"\n');
      const selected = run(args); reject(selected); assert(selected.stderr.includes('TABLE'));
      assert.deepEqual(read('absent.bin'), Buffer.alloc(0x8000, 0xea));
      write('absent.cfg', 'MEMORY { PRG: start=$8000, size=16, file=%O, fill=yes; } SEGMENTS { PROGRAM: load=PRG, type=ro; UNUSED: load=PRG, type=ro, align=16; TAIL: load=PRG, type=ro; }');
      write('absent.s', '.segment "PROGRAM"\n.byte $ea\n.segment "TAIL"\n.byte $bb\n');
      assert.equal(run(args).status, 0);
      const expected = Buffer.alloc(16); expected[0] = 0xea; expected[1] = 0xbb;
      assert.deepEqual(read('absent.bin'), expected);
    });
    test('ca65_extent_refusal', () => {
      const cases = [
        ['MEMORY { PRG: start=$8000, size=4, file=%O, fill=yes; NEXT: start=$9000, size=4, file=%O, fill=yes; } SEGMENTS { PROGRAM: load=PRG, type=ro; }', '.segment "PROGRAM"\n.res 5,$ea\n', 'PRG'],
        ['MEMORY { PRG: start=$8000, size=16, file=%O, fill=yes; } SEGMENTS { PROGRAM: load=PRG, type=ro; TABLE: load=PRG, type=ro, start=$8002; }', '.segment "PROGRAM"\n.res 3,$ea\n.segment "TABLE"\n.byte 1\n', 'TABLE'],
      ];
      for (const [cfg, source, detail] of cases) {
        write('extent.cfg', cfg); write('extent.s', source);
        const outputs = ['extent.bin', 'extent.sym', 'extent.lst', 'extent.debug198x'];
        for (const name of outputs) write(name, 'previous output');
        const p = run(['--dialect', 'ca65', '-C', 'extent.cfg', 'extent.s', '-o', outputs[0], '--sym=' + outputs[1], '--listing=' + outputs[2], '--debug=' + outputs[3]]);
        reject(p); assert(p.stderr.includes(detail), p.stderr);
        for (const name of outputs) assert.equal(read(name).toString(), 'previous output');
      }
    });
    test('quoted_data_native', () => {
      for (const [dialect, flags, fact] of quoted) for (const mode of ['human', 'json']) {
        write('quoted.s', fact.source); write('quoted.bin', 'previous image');
        const p = run(['--dialect', dialect, ...flags, '--message-format', mode, 'quoted.s', '-o', 'quoted.bin']);
        assert.equal(p.status, 0); assert.deepEqual(read('quoted.bin'), Buffer.from(fact.hex, 'hex'));
        if (mode === 'json') { assert.equal(p.stderr, ''); assert.deepEqual(JSON.parse(p.stdout).bytes, [...read('quoted.bin')]); }
        assert.equal(read('quoted.s').toString(), fact.source);
      }
    });
    test('quoted_data_format', () => {
      for (const [dialect, flags, fact] of quoted) {
        write('quoted.s', fact.source);
        assert.equal(run(['fmt', '--dialect', dialect, ...flags, 'quoted.s', '-o', 'formatted.s']).status, 0);
        assert.equal(run(['fmt', '--dialect', dialect, ...flags, 'formatted.s', '-o', 'twice.s']).status, 0);
        assert.deepEqual(read('formatted.s'), read('twice.s'));
        assert.equal(run(['--dialect', dialect, ...flags, 'formatted.s', '-o', 'formatted.bin']).status, 0);
        assert.deepEqual(read('formatted.bin'), Buffer.from(fact.hex, 'hex'));
        assert.equal(read('quoted.s').toString(), fact.source);
      }
    });
    test('conversion_quoted_data', () => {
      const fact = nativeCase('pasmo-quoted-semicolon', 'double'); write('convert.s', fact.source);
      assert.equal(run(['convert', '--from', 'pasmo', '--to', 'sjasmplus', 'convert.s', '-o', 'converted.s']).status, 0);
      assert.equal(run(['--dialect', 'sjasmplus', 'converted.s', '-o', 'converted.bin']).status, 0);
      assert.deepEqual(read('converted.bin'), Buffer.from(fact.hex, 'hex'));
      assert.equal(read('convert.s').toString(), fact.source);
    });
    test('conversion_file_scope', () => {
      write('child.inc', ' db 42\n'); write('data.bin', Buffer.from([42]));
      for (const [directive, file] of [['include', 'child.inc'], ['incbin', 'data.bin']]) {
        const source = ` org 8000h\n ${directive} "${file}"\n ret\n`; write('convert.s', source);
        assert.equal(run(['--dialect', 'pasmo', 'convert.s', '-o', 'included.bin']).status, 0);
        assert.deepEqual(read('included.bin'), Buffer.from('2ac9', 'hex'));
        write('converted.s', 'previous conversion');
        const p = run(['convert', '--from', 'pasmo', '--to', 'sjasmplus', 'convert.s', '-o', 'converted.s']); reject(p);
        assert.equal(p.stdout, ''); assert.match(p.stderr, /self-contained source/); assert(!p.stderr.includes('the CLI resolves'));
        assert.equal(read('converted.s').toString(), 'previous conversion'); assert.equal(read('convert.s').toString(), source);
      }
    });
    test('sjasmplus_decoded_escapes', () => {
      for (const name of ['\\n', '\\"', '\\\\']) {
        const fact = nativeCase('sjasmplus-escape-boundaries', name); write('escaped.s', fact.source);
        for (const cpu of ['z80', 'z80n']) for (const mode of ['human', 'json']) {
          const p = run(['--dialect', 'sjasmplus', '--cpu', cpu, '--message-format', mode, 'escaped.s', '-o', 'escaped.bin']);
          assert.equal(p.status, 0); assert.deepEqual(read('escaped.bin'), Buffer.from(fact.hex, 'hex'));
          assert.equal(read('escaped.s').toString(), fact.source);
          write('malformed-escape.s', ' db "unterminated' + String.fromCharCode(92) + '\n');
          reject(run(['--dialect', 'sjasmplus', '--cpu', cpu, '--message-format', mode, 'malformed-escape.s', '-o', 'escaped.bin']));
          assert.deepEqual(read('escaped.bin'), Buffer.from(fact.hex, 'hex'));
        }
      }
    });
    const project = path.join(root, 'upstream');
    try {
      for (const [name, digest] of Object.entries(sourceFiles)) {
        const response = await fetch(`https://raw.githubusercontent.com/kindjie/6502Assembly/${upstreamCommit}/${name}`, { signal: AbortSignal.timeout(30000) });
        assert(response.ok, `fixture acquisition: ${response.status}`); const bytes = Buffer.from(await response.arrayBuffer());
        assert(bytes.length <= 100000 && sha(bytes) === digest, `fixture checksum ${name}`);
        fs.mkdirSync(path.dirname(path.join(project, name)), { recursive: true }); fs.writeFileSync(path.join(project, name), bytes);
      }
      test('upstream_project', () => {
        assert.equal(run(['--dialect', 'acme', '--prg', 'helloworld.asm', '-o', 'hello.prg'], project).status, 0);
        const bytes = fs.readFileSync(path.join(project, 'hello.prg')); assert.equal(bytes.length, 2081);
        assert.equal(sha(bytes), '16e5a63289e474e84706517c592c0ba7f2ae86b5278d7d1e6eeb9a0c5ef205fc');
      });
    } catch (error) { results.push({ name: 'upstream_project', status: 'unavailable', reason: error.message }); }
    return { source_lock: sourceLock, source_lock_scope: sourceLock ? 'verified-producer-lock' : 'artifact-only', kind: 'candidate', version: identity.version, parsed_plan_sha256: sha(Buffer.from(JSON.stringify(plan))), checkout_commit: process.env.GITHUB_SHA || runChecked('git', ['rev-parse', 'HEAD'], { encoding: 'utf8', timeout: 10000 }).trim(),
      target, host: { platform: process.platform, arch: process.arch, release: os.release(), node: process.version, github_run: process.env.GITHUB_RUN_ID, github_attempt: process.env.GITHUB_RUN_ATTEMPT },
      archive: identity.archive, archive_sha256: sha(archive), executable_sha256: sha(executable), checksum_mutation_rejected: true,
      fixture: { commit: upstreamCommit, files: sourceFiles }, qualified: qualifies(results), results };
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
}
module.exports = { audit, packageIdentity, nativeTarget, binaryTarget, qualifies, requiredCases, targets };
if (require.main === module) {
  const [plan, target, dist, output, ...flags] = process.argv.slice(2);
  const control = flags.find(f => f.startsWith('--control-'));
  (async () => {
    assert(plan && target && dist && output, 'usage: node test-candidate-package.cjs <plan.json> <target> <distrib> <report.json> [--control-wrong-bytes]');
    const report = await audit(JSON.parse(fs.readFileSync(plan, 'utf8')), target, path.resolve(dist), { corruptOutput: control === '--control-wrong-bytes', corruptArchive: control === '--control-corrupt-archive', requireSourceLock: flags.includes('--require-source-lock'), source: flags.includes('--published-source') && fs.existsSync(path.join(dist, 'published-source.json')) ? JSON.parse(fs.readFileSync(path.join(dist, 'published-source.json'), 'utf8')) : null });
    fs.mkdirSync(path.dirname(path.resolve(output)), { recursive: true }); fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify({ target, qualified: report.qualified, cases: report.results.length, failures: report.results.filter(r => r.status !== 'pass').map(r => r.name) }));
    if (!report.qualified) process.exitCode = 1;
  })().catch(error => {
    if (output) {
      fs.mkdirSync(path.dirname(path.resolve(output)), { recursive: true });
      fs.writeFileSync(output, JSON.stringify({ kind: 'candidate', target, qualified: false, results: [], error: error.message }, null, 2) + '\n');
    }
    console.error(error.stack); process.exitCode = 1;
  });
}
