# Numeric DEFINE substitution (#734)

The captured SjASMPlus 1.21.0 fixture contains 136 real executions: 102 accepted
sources and 34 refusals. The bounded `probe.py` retains the tool/version/hash,
argv, source, diagnostics and bytes, and removes the old native output before
every invocation. `red.log.gz` retains the initial wrong-byte failures: `$FF`
emitted `ff000000` instead of native `07000000`, and the CLI success control
emitted `ffff` instead of `0707`.

The old DEFINE scanner copied complete digit-led and hexadecimal runs without
looking for identifiers. Native default-mode substitution instead starts an
identifier at an ASCII letter or underscore, including after digits or numeric
sigils, and consumes that whole identifier. Thus `DEFINE FF 7` changes `$FF`
and `#FF` to `$7` and `#7`; `DEFINE X 3` changes `1X` to `13`. Defining `FF`
does not change `0xFF`, because its identifier is `xFF`. Defining `xFF` does.
Names remain case-sensitive, and defining `X` does not change `X0` or `X_0`.

The existing scanner now follows those boundaries. Its identifier tails include
the native punctuation, so a shorter name cannot incorrectly replace part of a
longer identifier. It protects complete single- and double-quoted text, rather
than just three-character `'c'` shapes, and double-quoted backslashes protect the
next character as native does. Numeric apostrophes and AF' do not start strings.
Line comments remain untouched. The expression evaluator, number parser, public
APIs, AST and Pasmo path are unchanged.

The primary implementation is [SjASMPlus 1.21.0 ReplaceDefineInternal](https://github.com/z00m128/sjasmplus/blob/v1.21.0/sjasm/parser.cpp#L444),
with identifier predicates in [reader.cpp](https://github.com/z00m128/sjasmplus/blob/v1.21.0/sjasm/reader.cpp#L864).
This correction follows its default whole-identifier mode; it does not add the
optional subword-substitution mode or DEFARRAY support.

Native chains of twenty replacements succeed; twenty-one fail even when the
last replacement is already a number. The old private loop allowed more passes
and treated a self-replacement as successful completion. The loop now permits
twenty replacements plus the unchanged detection pass and refuses self cycles.
The existing size guard and error contract remain. Captured controls include
20/21 boundary chains, self/indirect cycles and growing recursion.

The fixture also covers prefixes, suffixes, numeric separators, adjacent
operators, quoted strings/characters/escapes, UTF-8, AF', DEFINE chains, macros,
STRUCT defaults and invalid replacement results. Each accepted case requires
byte/symbol-preserving, idempotent formatter output in both CPU entry points.
CLI controls verify included line 5 in human/JSON diagnostics, retain the source,
and preserve five existing output artifacts on refusal. The mandatory package
case replays all 136 sources against the packaged executable.

A separate existing parser gap was measured: native `dd 'FF'` emits `46460000`,
but Asm198x refuses packed single-quoted literals even without DEFINE. It is
kept as a separate follow-up, not silently asserted as supported by this fix.
The numeric fixture uses existing single-character and double-quoted grammar;
a private scanner invariant additionally prevents a protected `'FF'` from
being rewritten to an accepted, incorrect `'7'` while that parser gap remains.

Final local validation: 1,501 workspace tests passed, 38 explicitly ignored.
Clippy with all targets/Lua and warnings denied, formatting, generated docs,
Ruff and all 51 Node package/workflow checks passed. Their logs are retained.
