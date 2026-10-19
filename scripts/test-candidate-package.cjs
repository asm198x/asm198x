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
requiredCases.push('ca65_used_extents');
requiredCases.push('sjasmplus_logical_expressions');
requiredCases.push('sjasmplus_shift_widths');
requiredCases.push('sjasmplus_digit_separators');
requiredCases.push('sjasmplus_remainder');
requiredCases.push('sjasmplus_define_numbers');
requiredCases.push('sjasmplus_display_format');
requiredCases.push('sjasmplus_savenex');
requiredCases.push('sjasmplus_forward_equ');
requiredCases.push('ca65_4510');
requiredCases.push('ca65_4510_bit_branches');
requiredCases.push('ca65_4510_phd');
requiredCases.push('ca65_address_size');
requiredCases.push('ca65_scoped_constants');
requiredCases.push('ca65_scope_repetition');
requiredCases.push('ca65_include_streams');
requiredCases.push('ca65_text_capture');
requiredCases.push('ca65_macro_introspection');
requiredCases.push('ca65_constant_names');
requiredCases.push('ca65_cheap_constants');
requiredCases.push('ca65_scope_anchors');
requiredCases.push('ca65_reserved_constants');
requiredCases.push('ca65_label_delimiters');
requiredCases.push('ca65_macro_definition_labels');
requiredCases.push('ca65_record_allocators');
requiredCases.push('ca65_deferred_aliases');
requiredCases.push('ca65_scope_binding');
requiredCases.push('ca65_unused_cycles');
requiredCases.push('ca65_enum_expressions');
requiredCases.push('ca65_zp_output');
requiredCases.push('ca65_bss_warnings');
requiredCases.push('ca65_scoped_exports');
requiredCases.push('ca65_org_labels');
requiredCases.push('ca65_origin_bounds');
requiredCases.push('ca65_hex_suffix');
requiredCases.push('ca65_absolute_counter');
requiredCases.push('ca65_absolute_debug');
requiredCases.push('ca65_numeric_conversion');
requiredCases.push('ca65_numeric_admission');
requiredCases.push('ca65_repeat_end');
requiredCases.push('ca65_format_includes');
requiredCases.push('ca65_end_tokens');
requiredCases.push('ca65_reserved_labels');
requiredCases.push('pasmo_character_literals');
requiredCases.push('sjasmplus_packed_single');
requiredCases.push('ca65_format_end');
requiredCases.push('ca65_labelled_end');
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
    test('sjasmplus_logical_expressions', () => {
      const facts = require('../crates/asm198x/tests/fixtures/sjasmplus-logical-expressions.json');
      assert.equal(facts.cases.length, 190);
      for (const fact of facts.cases) {
        write('logical.asm', fact.source); write('logical.bin', 'previous image');
        const p = run(['--dialect', 'sjasmplus', 'logical.asm', '-o', 'logical.bin']);
        if (fact.exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('logical.bin'), Buffer.from(fact.hex, 'hex'), fact.name);
        } else {
          reject(p); assert.equal(read('logical.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('sjasmplus_shift_widths', () => {
      const facts = require('../crates/asm198x/tests/fixtures/sjasmplus-shift-widths.json');
      assert.equal(facts.cases.length, 646);
      for (const fact of facts.cases) {
        write('shifts.asm', fact.source); write('shifts.bin', 'previous image');
        const p = run(['--dialect', 'sjasmplus', 'shifts.asm', '-o', 'shifts.bin']);
        if (fact.exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('shifts.bin'), Buffer.from(fact.hex, 'hex'), fact.name);
        } else {
          reject(p); assert.equal(read('shifts.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('sjasmplus_digit_separators', () => {
      const facts = require('../crates/asm198x/tests/fixtures/sjasmplus-digit-separators.json');
      assert.equal(facts.cases.length, 65);
      for (const fact of facts.cases) {
        write('separators.asm', fact.source); write('separators.bin', 'previous image');
        const p = run(['--dialect', 'sjasmplus', 'separators.asm', '-o', 'separators.bin']);
        if (fact.exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('separators.bin'), Buffer.from(fact.hex, 'hex'), fact.name);
        } else {
          reject(p); assert.equal(read('separators.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('sjasmplus_remainder', () => {
      const facts = require('../crates/asm198x/tests/fixtures/sjasmplus-remainder.json');
      assert.equal(facts.cases.length, 236);
      for (const fact of facts.cases) {
        write('remainder.asm', fact.source); write('remainder.bin', 'previous image');
        const p = run(['--dialect', 'sjasmplus', 'remainder.asm', '-o', 'remainder.bin']);
        if (fact.exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('remainder.bin'), Buffer.from(fact.hex, 'hex'), fact.name);
        } else {
          reject(p); assert.equal(read('remainder.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('sjasmplus_forward_equ', () => {
      const facts = require('../crates/asm198x/tests/fixtures/sjasmplus-forward-equ.json');
      assert.equal(facts.cases.length, 86);
      for (const [name, hex] of Object.entries(facts.binary_files)) write(name, Buffer.from(hex, 'hex'));
      for (const [name, text] of Object.entries(facts.text_files)) write(name, text);
      for (const fact of facts.cases) for (const cpu of ['z80', 'z80n']) {
        write('equ.asm', fact.source); write('equ.bin', 'previous image');
        const p = run(['--dialect', 'sjasmplus', '--cpu', cpu, 'equ.asm', '-o', 'equ.bin']);
        if (fact.exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('equ.bin'), Buffer.from(fact.hex, 'hex'), fact.name);
        } else {
          reject(p); assert.equal(read('equ.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('sjasmplus_define_numbers', () => {
      const facts = require('../crates/asm198x/tests/fixtures/sjasmplus-define-numbers.json');
      assert.equal(facts.cases.length, 136);
      for (const fact of facts.cases) {
        write('defines.asm', fact.source); write('defines.bin', 'previous image');
        const p = run(['--dialect', 'sjasmplus', 'defines.asm', '-o', 'defines.bin']);
        if (fact.exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('defines.bin'), Buffer.from(fact.hex, 'hex'), fact.name);
        } else {
          reject(p); assert.equal(read('defines.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('sjasmplus_display_format', () => {
      const facts = require('../crates/asm198x/tests/fixtures/sjasmplus-display-format.json');
      assert.equal(facts.cases.length, 132);
      for (const fact of facts.cases) {
        write('display.asm', fact.source); write('display.bin', 'previous image');
        const p = run(['--dialect', 'sjasmplus', '--message-format', 'json', 'display.asm', '-o', 'display.bin']);
        if (fact.exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('display.bin'), Buffer.from(fact.hex, 'hex'), fact.name);
          const expected = fact.stderr === '' ? [] : fact.stderr.slice(2, -1).split('\n> ');
          const notes = JSON.parse(p.stdout).warnings.filter(w => w.kind === 'Note').map(w => w.message);
          assert.deepEqual(notes, expected, fact.name + ': DISPLAY text');
        } else {
          reject(p); assert.equal(read('display.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('sjasmplus_savenex', () => {
      const facts = require('../crates/asm198x/tests/fixtures/sjasmplus-savenex.json');
      assert.equal(facts.cases.length, 70);
      for (const fact of facts.cases) {
        for (const name of ['out.nex', 'other.nex']) {
          const file = path.join(root, name); if (fs.existsSync(file)) fs.unlinkSync(file);
        }
        write('nex.asm', fact.source); write('nex.bin', 'previous image');
        const p = run(['--dialect', 'sjasmplus', '--message-format', 'json', 'nex.asm', '-o', 'nex.bin']);
        if (fact.exit === 0 && fact.name !== 'open_8') {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          for (const output of fact.outputs) {
            assert.deepEqual(read(output.name === 'raw.bin' ? 'nex.bin' : output.name), Buffer.from(output.hex, 'hex'), fact.name + ': ' + output.name);
          }
          const result = JSON.parse(p.stdout);
          assert.equal((result.artifacts || []).length, fact.name === 'duplicate_name' ? 2 : fact.outputs.filter(o => o.name !== 'raw.bin').length);
          if (fact.name === 'duplicate_name') {
            const first = facts.cases.find(c => c.name === 'implicit_close').outputs.find(o => o.name === 'out.nex');
            assert.deepEqual(Buffer.from(result.artifacts[0].bytes), Buffer.from(first.hex, 'hex'), 'first ordered save');
          }
          for (const artifact of fact.name === 'duplicate_name' ? result.artifacts.slice(-1) : (result.artifacts || [])) {
            assert.equal(artifact.format, 'nex');
            const expected = fact.outputs.find(o => o.name === artifact.name);
            assert(expected, fact.name + ': unexpected artifact');
            assert.deepEqual(Buffer.from(artifact.bytes), Buffer.from(expected.hex, 'hex'), fact.name + ': JSON artifact');
          }
        } else {
          reject(p); assert.equal(read('nex.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_4510', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-4510.json');
      assert.equal(facts.cases.length, 139);
      for (const [name, text] of Object.entries(facts.files)) write(name, text);
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const p = run(['--dialect', 'ca65', '-C', 'cpu.cfg', 'cpu.s', '-o', 'cpu.bin']);
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_4510_phd', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-4510-phd.json');
      assert.equal(facts.cases.length, 99);
      for (const [name, text] of Object.entries(facts.files)) write(name, text);
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const dialect = fact.name.startsWith('65816_') ? '65816' : 'ca65';
        const args = dialect === '65816' ? [] : ['-C', 'cpu.cfg'];
        const p = run(['--dialect', dialect, ...args, 'cpu.s', '-o', 'cpu.bin']);
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_address_size', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-address-size.json');
      assert.equal(facts.cases.length, 113);
      for (const [name, text] of Object.entries(facts.files)) write(name, text);
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const dialect = fact.name.startsWith('65816_') ? '65816' : 'ca65';
        const args = dialect === '65816' ? [] : ['-C', 'cpu.cfg'];
        const p = run(['--dialect', dialect, ...args, 'cpu.s', '-o', 'cpu.bin']);
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_scoped_constants', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-scoped-constants.json');
      assert.equal(facts.cases.length, 49);
      for (const [name, text] of Object.entries(facts.files)) write(name, text);
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_scope_repetition', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-scope-repetition.json');
      assert.equal(facts.cases.length, 35);
      for (const [name, text] of Object.entries(facts.files)) write(name, text);
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_include_streams', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-include-streams.json');
      assert.equal(facts.cases.length, 44);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_text_capture', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-text-capture.json');
      assert.equal(facts.cases.length, 51);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_macro_introspection', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-macro-introspection.json');
      assert.equal(facts.cases.length, 35);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_constant_names', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-constant-names.json');
      assert.equal(facts.cases.length, 21);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_cheap_constants', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-cheap-constants.json');
      assert.equal(facts.cases.length, 36);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_scope_anchors', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-scope-anchors.json');
      assert.equal(facts.cases.length, 102);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_reserved_constants', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-reserved-constants.json');
      assert.equal(facts.cases.length, 378);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_label_delimiters', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-label-delimiters.json');
      assert.equal(facts.cases.length, 24);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_macro_definition_labels', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-macro-definition-labels.json');
      assert.equal(facts.cases.length, 27);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_record_allocators', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-record-allocators.json');
      assert.equal(facts.cases.length, 386);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_deferred_aliases', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-deferred-aliases.json');
      assert.equal(facts.cases.length, 63);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_scope_binding', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-scope-binding.json');
      assert.equal(facts.cases.length, 312);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_unused_cycles', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-unused-cycles.json');
      assert.equal(facts.cases.length, 164);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_enum_expressions', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-enum-expressions.json');
      assert.equal(facts.cases.length, 136);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_zp_output', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-zp-output.json');
      assert.equal(facts.cases.length, 216);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_scoped_exports', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-scoped-exports.json');
      assert.equal(facts.cases.length, 222);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_hex_suffix', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-hex-suffix.json');
      assert.equal(facts.cases.length, 30);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      const cases = facts.cases.map(fact => ({ ...fact, dialect: 'ca65' }));
      for (const dialect of ['65816', 'huc6280']) {
        for (const fact of facts.cases.filter(f => ['constant_0FFh', 'difference_100000000h'].includes(f.name))) cases.push({ ...fact, dialect });
      }
      for (const fact of cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', fact.dialect, ...(fact.dialect === 'ca65' ? ['-C', 'cpu.cfg'] : []), input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', fact.dialect, 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', fact.dialect, 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_absolute_counter', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-absolute-counter.json');
      assert.equal(facts.cases.length, 63);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_numeric_conversion', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-numeric-conversion.json');
      assert.equal(facts.cases.length, 186);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', fact.dialect, ...(fact.dialect === 'ca65' ? ['-C', 'cpu.cfg'] : []), input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', fact.dialect, 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', fact.dialect, 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_numeric_admission', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-numeric-admission.json');
      assert.equal(facts.cases.length, 317);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', fact.dialect, ...(fact.dialect === 'ca65' ? ['-C', 'cpu.cfg'] : []), input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          if (fact.formatter_gap) continue;
          const before = shape();
          const formatted = run(['fmt', '--dialect', fact.dialect, 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', fact.dialect, 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_repeat_end', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-repeat-end.json');
      assert.equal(facts.cases.length, 22);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', fact.dialect, ...(fact.dialect === 'ca65' ? ['-C', 'cpu.cfg'] : []), input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          if (fact.formatter_gap) continue;
          const before = shape();
          const formatted = run(['fmt', '--dialect', fact.dialect, 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', fact.dialect, 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_format_includes', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-format-includes.json');
      assert.equal(facts.cases.length, 63);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', fact.dialect, ...(fact.dialect === 'ca65' ? ['-C', 'cpu.cfg'] : []), input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          if (fact.formatter_gap) continue;
          const before = shape();
          const formatted = run(['fmt', '--dialect', fact.dialect, 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', fact.dialect, 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
          const formatted = run(['fmt', '--dialect', fact.dialect, 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', fact.dialect, 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 1, fact.name + ': ' + roundtrip.stderr);
          assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);

        }
      }
    });
    test('ca65_end_tokens', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-end-tokens.json');
      assert.equal(facts.cases.length, 25);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source); write('cpu.bin', 'previous image');
        const p = run(['--dialect', 'ca65', '-C', 'cpu.cfg', 'cpu.s', '-o', 'cpu.bin', '--debug=scope.debug198x']);
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr);
          assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_reserved_labels', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-reserved-labels.json');
      assert.equal(facts.cases.length, 113);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source); write('cpu.bin', 'previous image');
        const p = run(['--dialect', 'ca65', '-C', 'cpu.cfg', 'cpu.s', '-o', 'cpu.bin', '--debug=scope.debug198x']);
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          if (fact.formatter_gap) continue;
          const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = run(['--dialect', 'ca65', '-C', 'cpu.cfg', 'scope-formatted.s', '-o', 'cpu.bin', '--debug=scope.debug198x']);
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and sections');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr);
          assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_labelled_end', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-labelled-end.json');
      assert.equal(facts.cases.length, 115);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source); write('cpu.bin', 'previous image');
        const p = run(['--dialect', 'ca65', '-C', 'cpu.cfg', 'cpu.s', '-o', 'cpu.bin', '--debug=scope.debug198x']);
        if (fact.native.at(-1).exit === 0 && !fact.assembly_gaps) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = run(['--dialect', 'ca65', '-C', 'cpu.cfg', 'scope-formatted.s', '-o', 'cpu.bin', '--debug=scope.debug198x']);
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and sections');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr);
          assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('pasmo_character_literals', () => {
      const facts = require('../crates/asm198x/tests/fixtures/pasmo-character-literals.json');
      assert.equal(facts.cases.length, 6);
      for (const cpu of ['z80', 'z80n']) {
        for (const fact of facts.cases) {
          write('cpu.s', fact.source); write('cpu.bin', 'previous image');
          const assemble = input => run(['--dialect', 'pasmo', '--cpu', cpu, input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
          const p = assemble('cpu.s');
          if (fact.native.at(-1).exit === 0) {
            assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
            assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
            const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
            const before = shape();
            const formatted = run(['fmt', '--dialect', 'pasmo', 'cpu.s']);
            assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
            write('scope-formatted.s', formatted.stdout);
            const again = run(['fmt', '--dialect', 'pasmo', 'scope-formatted.s']);
            assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
            assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
            const roundtrip = assemble('scope-formatted.s');
            assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
            assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
            assert.deepEqual(shape(), before, fact.name + ': formatted symbols and sections');
          } else {
            assert.equal(p.status, 1, fact.name + ': ' + p.stderr);
            assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
          }
        }
      }
    });
    test('sjasmplus_packed_single', () => {
      const facts = require('../crates/asm198x/tests/fixtures/sjasmplus-packed-single.json');
      assert.equal(facts.cases.length, 293);
      for (const cpu of ['z80']) {
        for (const fact of facts.cases) {
          write('cpu.s', fact.source); write('cpu.bin', 'previous image');
          const assemble = input => run(['--dialect', 'sjasmplus', '--cpu', cpu, input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
          const p = assemble('cpu.s');
          if (fact.native.at(-1).exit === 0) {
            assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
            assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
            const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
            const before = shape();
            const formatted = run(['fmt', '--dialect', 'sjasmplus', 'cpu.s']);
            assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
            write('scope-formatted.s', formatted.stdout);
            const again = run(['fmt', '--dialect', 'sjasmplus', 'scope-formatted.s']);
            assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
            assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
            const roundtrip = assemble('scope-formatted.s');
            assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
            assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
            assert.deepEqual(shape(), before, fact.name + ': formatted symbols and sections');
          } else {
            assert.equal(p.status, 1, fact.name + ': ' + p.stderr);
            assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
          }
        }
      }
    });
    test('ca65_format_end', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-format-end.json');
      assert.equal(facts.cases.length, 26);
      const lexical = text => {
        let tokens = ''; const comments = [];
        for (const line of text.split(/\r?\n/)) {
          let quote = null;
          for (let i = 0; i < line.length; i++) {
            const ch = line[i];
            if (quote) { tokens += ch; if (ch === quote) quote = null; }
            else if (ch === ';') { comments.push(line.slice(i + 1)); break; }
            else if (ch === "'" || ch === '"') { quote = ch; tokens += ch; }
            else if (!/\s/.test(ch)) tokens += ch;
          }
        }
        return { tokens, comments };
      };
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
        assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
        assert.deepEqual(lexical(formatted.stdout), lexical(fact.source), fact.name + ': complete source');
        write('scope-formatted.s', formatted.stdout);
        const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
        assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
        assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
        write('cpu.bin', 'previous image');
        const p = run(['--dialect', 'ca65', '-C', 'cpu.cfg', 'scope-formatted.s', '-o', 'cpu.bin']);
        const accepted = fact.native.at(-1).exit === 0 && !fact.formatted_assembly_gap;
        assert.equal(p.status, accepted ? 0 : 1, fact.name + ': ' + p.stderr);
        if (accepted) assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': native bytes');
        else assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
      }
    });
    test('ca65_absolute_debug', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-absolute-debug.json');
      assert.equal(facts.cases.length, 39);
      let labelCount = 0, lineCount = 0;
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x', '--listing=scope.lst', '--listing-json=scope.lst.json']);
      const records = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s));
      const shape = rows => rows.filter(r => r.t === 'symbol' || r.t === 'section');
      const relative = name => {
        name = name.replaceAll('\\', '/');
        return ['cpu.s', ...Object.keys(facts.files)].find(p => name === p || name.endsWith('/' + p))?.replace(/^cpu\.s$/, 'input.s') ?? name;
      };
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source); write('cpu.bin', 'previous image');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit !== 0) {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr); assert.equal(read('cpu.bin').toString(), 'previous image'); continue;
        }
        assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
        const bytes = read('cpu.bin'); assert.deepEqual(bytes, Buffer.from(fact.native_hex, 'hex'), fact.name);
        const rows = records();
        const sections = new Map(rows.filter(r => r.t === 'section').map(r => [r.id, r]));
        const labels = new Map(rows.filter(r => r.t === 'symbol' && r.kind === 'label').map(r => [r.name, r]));
        assert.deepEqual([...labels.keys()].sort(), Object.keys(fact.symbol_values).sort(), fact.name + ': label kinds');
        for (const [name, value] of Object.entries(fact.symbol_values)) {
          labelCount++;
          const label = labels.get(name), section = sections.get(label.section);
          assert(section); assert.equal(label.space, undefined); assert.equal(section.space, undefined);
          assert.equal(section.base == null ? null : section.base + label.offset, Object.hasOwn(fact.non_cpu_symbols, name) ? null : value, fact.name + ': ' + name);
        }
        for (const section of sections.values()) {
          assert(Object.hasOwn(facts.config_ids, section.name));
          if (section.id < 7) assert.equal(section.id, facts.config_ids[section.name]);
        }
        const constants = new Map(rows.filter(r => r.t === 'symbol' && r.kind === 'const').map(r => [r.name, r.value]));
        assert.deepEqual([...constants].sort(), Object.entries(fact.constant_values).sort(), fact.name + ': constants');
        const listing = JSON.parse(read('scope.lst.json'));
        const projected = listing.lines.map(l => ({file: relative(l.file), line: l.line, file_offset: l.file_offset, length: l.bytes, cycles: l.cycles ?? null}));
        lineCount += projected.length;
        const sorted = values => values.map(v => JSON.stringify(v)).sort();
        assert.deepEqual(sorted(projected), sorted(fact.expected_lines), fact.name + ': physical source spans and cycles');
        for (const line of listing.lines) {
          assert(fact.physical_lines.some(s => s.file === relative(line.file) && s.line === line.line && s.file_offset != null && s.file_offset <= line.file_offset && s.file_offset + s.length >= line.file_offset + line.bytes), fact.name + ': native physical span');
          if (fact.name === 'pc_data_lines') assert.equal(line.address, bytes.readUInt16LE(line.file_offset));
        }
        for (const name of [...fact.no_file_symbols, ...fact.metadata_only_symbols]) {
          const label = labels.get(name);
          assert(listing.lines.every(l => l.section !== label.section));
          if (fact.no_file_symbols.includes(name)) assert.equal(listing.sections.find(s => s.id === label.section).file_offset, null);
        }
        for (const [name, cost] of Object.entries(fact.cycle_costs)) {
          const actual = listing.labels.find(l => l.name === name); assert(actual);
          assert.deepEqual(actual.cycles, {min: cost.min, max: cost.max}); assert.equal(actual.bytes, cost.bytes);
          assert(read('scope.lst').toString().split('\n').some(line => line.startsWith('  ' + name + ' (+$') && line.endsWith(': ' + cost.bytes + ' bytes, ' + cost.max + ' cycles (straight-line)')));
        }
        if (fact.name === 'no_origin') {
          const normalized = rows.map(r => r.t === 'header' ? {...r, sources: r.sources.map(relative)} : r.t === 'line' ? {...r, file: relative(r.file)} : r);
          const baseline = structuredClone(facts.no_origin_records);
          baseline[0].tool_version = normalized[0].tool_version;
          assert.deepEqual(normalized, baseline);
        }
        const before = shape(rows);
        const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']); assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
        write('scope-formatted.s', formatted.stdout);
        const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']); assert.equal(again.status, 0); assert.equal(again.stdout, formatted.stdout);
        const roundtrip = assemble('scope-formatted.s'); assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
        assert.deepEqual(read('cpu.bin'), bytes); assert.deepEqual(shape(records()), before, fact.name + ': formatted coordinates');
      }
      assert.equal(labelCount, 121); assert.equal(lineCount, 213);
      let budgets = 0;
      for (const fact of facts.cases) {
        for (const [name, cost] of Object.entries(fact.cycle_costs)) {
          if (name.includes('::')) continue; // Existing budget grammar excludes qualified names.
          budgets++;
          write('cpu.cfg', fact.config);
          for (const limit of [cost.max, cost.max - 1]) {
            write('cpu.s', fact.source + '; asm198x: cycles(' + name + ') <= ' + limit + '\n');
            const p = assemble('cpu.s'); assert.equal(p.status, limit === cost.max ? 0 : 1, fact.name + ': ' + p.stderr);
            if (limit !== cost.max) assert(p.stderr.includes('exceeds its cycle budget'));
          }
        }
      }
      assert.equal(budgets, 9);
      const unmapped = facts.cases.find(c => c.name === 'no_file');
      write('cpu.cfg', unmapped.config); write('cpu.s', unmapped.source + '; asm198x: cycles(buffer) <= 0\n');
      const unknown = assemble('cpu.s'); assert.equal(unknown.status, 1); assert(unknown.stderr.includes('names no label with instructions'));
    });
    test('ca65_origin_bounds', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-origin-bounds.json');
      assert.equal(facts.cases.length, 7);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_org_labels', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-org-labels.json');
      assert.equal(facts.cases.length, 145);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const before = shape();
          if (fact.name === 'org_0_global_True') {
            const symbol = before.find(r => r.t === 'symbol' && r.name === 'item');
            assert(symbol, 'item symbol');
            const section = before.find(r => r.t === 'section' && r.id === symbol.section);
            assert(section, 'item section');
            assert.equal(section.base + symbol.offset, read('cpu.bin').readUInt16LE(0), 'label/debug agreement');
          }
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_bss_warnings', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-bss-warnings.json');
      assert.equal(facts.cases.length, 272);
      for (const [name, text] of Object.entries(facts.files)) {
        fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true }); write(name, text);
      }
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const assemble = input => run(['--dialect', 'ca65', '-C', 'cpu.cfg', input, '-o', 'cpu.bin', '--debug=scope.debug198x', '--message-format', 'json']);
        const shape = () => read('scope.debug198x').toString().trim().split('\n').map(s => JSON.parse(s)).filter(r => r.t === 'symbol' || r.t === 'section');
        const p = assemble('cpu.s');
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
          const count = result => JSON.parse(result.stdout).warnings.filter(w => w.message.includes('contains initialized data')).length;
          assert.equal(count(p), fact.native_warning_count, fact.name + ': warning count');
          const before = shape();
          const formatted = run(['fmt', '--dialect', 'ca65', 'cpu.s']);
          assert.equal(formatted.status, 0, fact.name + ': ' + formatted.stderr);
          write('scope-formatted.s', formatted.stdout);
          const again = run(['fmt', '--dialect', 'ca65', 'scope-formatted.s']);
          assert.equal(again.status, 0, fact.name + ': ' + again.stderr);
          assert.equal(again.stdout, formatted.stdout, fact.name + ': idempotence');
          const roundtrip = assemble('scope-formatted.s');
          assert.equal(roundtrip.status, 0, fact.name + ': ' + roundtrip.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name + ': formatted bytes');
          assert.equal(count(roundtrip), fact.native_warning_count, fact.name + ': formatted warnings');
          assert.deepEqual(shape(), before, fact.name + ': formatted symbols and addresses');
        } else {
          assert.equal(p.status, 1, fact.name + ': ' + p.stderr); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_4510_bit_branches', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-4510-bit-branches.json');
      assert.equal(facts.cases.length, 148);
      for (const [name, text] of Object.entries(facts.files)) write(name, text);
      for (const fact of facts.cases) {
        write('cpu.cfg', fact.config); write('cpu.s', fact.source);
        write('cpu.bin', 'previous image');
        const p = run(['--dialect', 'ca65', '-C', 'cpu.cfg', 'cpu.s', '-o', 'cpu.bin']);
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('cpu.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
        } else {
          reject(p); assert.equal(read('cpu.bin').toString(), 'previous image', fact.name);
        }
      }
    });
    test('ca65_used_extents', () => {
      const facts = require('../crates/asm198x/tests/fixtures/ca65-used-extents.json');
      assert.equal(facts.cases.length, 51);
      for (const fact of facts.cases) {
        write('used.cfg', fact.config); write('used.s', fact.source);
        write('used.bin', 'previous image');
        const p = run(['--dialect', 'ca65', '-C', 'used.cfg', 'used.s', '-o', 'used.bin']);
        if (fact.native.at(-1).exit === 0) {
          assert.equal(p.status, 0, fact.name + ': ' + p.stderr);
          assert.deepEqual(read('used.bin'), Buffer.from(fact.native_hex, 'hex'), fact.name);
        } else {
          reject(p);
          assert.equal(read('used.bin').toString(), 'previous image', fact.name);
        }
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
