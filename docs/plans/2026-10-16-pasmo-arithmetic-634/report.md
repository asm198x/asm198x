# Pasmo arithmetic correction (#634)

The source fix makes the two reported divisions emit native `00 00 00 00`
rather than the released CLI's `00 80 00 80`. Pasmo and PasmoNext now normalize
atoms and each operation to unsigned 16-bit values before the next operation
uses them. This is a source correction based on `93a5984a84bcb30af8a09bccf1817e5f24b59a33`,
not a new release or qualification of a published executable.

## Cause and correction

The Pasmo-specific Pratt parser already masked bitwise/truth results, but
left literals, unary minus and arithmetic intermediates on the shared i64
path. Thus `65536/2` divided 65536 before any truncation. The native assembler
first treats the operand as zero. `(65535+2)/2` likewise needs the addition to
wrap before division. Merely masking the final data/count result cannot fix it.

The correction stays in Pasmo lowering and uses existing expression nodes:

- Normalize atoms, negation and every binary result. Preserve native prefix
  precedence and refuse unparenthesized signs where the native grammar does.
- Use the measured shift-count behavior (32 and 64 act as zero), without
  changing the shared i64 evaluator. Admit native `SHL`/`SHR` and the logical
  spellings in data expressions through the existing Pasmo token mapping.
- Interpret a word-valued index displacement as signed before the engine's
  existing byte-range check. Native `65535` means `-1`; `128` remains invalid.
- Accept wrapped EQU bindings. The former test/comment claiming `$10000`
  must be refused contradicted fresh PasmoNext 0.1.3 observations; its value
  is zero. Correct that assumption rather than preserving its old test.
- Preserve an already-known bare entry symbol in the word domain, so `END
  start` retains its existing debug entry identity. No debug schema, shared
  evaluator, golden fixture or non-Pasmo arithmetic change is made.

## Evidence

The [native facts](../../../crates/asm198x/tests/fixtures/pasmo-arithmetic.json)
retain 194 exact sources, commands, exits, streams, symbols and bytes:
182 accepted cases and 12 refusals. They cover literal/intermediate overflow,
unary signs, unsigned division, EQU, comparisons, short-circuit controls,
shift boundaries, forward/backward labels, live PC, IF/REPT/DEFS, instruction
operands and signed index limits. The independent reference is PasmoNext
v0.1.3, executable SHA-256
`e9628f5c32f3c74a963c649781e6cb87e9eaeb4bbfe7b52d3de27f4986bbf136`.
The producer records its macOS ARM64/Python identity. Repeat with:

```sh
python3 docs/plans/2026-10-16-pasmo-arithmetic-634/probe.py \
  /path/to/pasmo /new/native-facts.json
```

The normal integration suite replays both frontends against all facts, checks
182 formatter idempotence cases and their native bytes through both frontends,
and executes 104 CLI checks: 56 positive byte comparisons and 48 rejection/
previous-output/source-preservation checks across human/JSON modes. Explicit
counts prevent a changed filter or an empty cohort from passing silently.

Fresh native replay passed all 194 cases separately. A fresh SjASMPlus 1.21.0
[isolation fact](../../../crates/asm198x/tests/fixtures/pasmo-arithmetic-isolation.json)
emits `00 80 00 80` for these expressions; both SjASMPlus CPU entry points
retain that wider arithmetic. Its executable SHA-256 is
`454bfa33058d5a0f5323db1ee65bbdd2b5871f7d697c328e2ee1f1a950f0d78b`.

## Verification and failures caught

Before production edits, all three initial regressions failed on the original
`65536/2` mismatch: actual `[0, 128]`, native `[0, 0]` (`red.log.gz`). The first
full workspace run caught `z80-spectrum: sidecar drifted from the committed
fixture` (`workspace-red.log.gz`): masking a bare entry symbol produced
`@entry` instead of upgrading `start`. The parser now preserves known
word-valued bare entry identity; all seven existing debug fixture tests pass
unchanged. A mistaken SjASMPlus function name in the new test was corrected
before executable verification.

The corrected Lua-enabled workspace run passes **1,454 tests, zero failures,
36 ignored** (`workspace-green.log.gz`). Final focused/native replay passes all
five tests, including the normally ignored external-tool replay
(`native-final.log.gz`). Strict workspace/all-target Clippy with Lua, rustfmt,
Ruff checks/formatting and generated documentation checks pass. The final
focused rerun also verifies the added nonempty/exact-count guards; the only
later production-file edit removes an incorrect comment. Logs are retained
beside this report.

## Limits and remaining work

This is a bounded arithmetic correction, not complete Pasmo source/warning
parity. Native warning text is retained, including pre-existing index-minus
warnings; matching that warning text is not claimed. Shift behavior is measured
against the identified reference executable, not every native compiler build.
Address-space output wrapping (#631), ca65 layout validation (#609), SjASMPlus
word operators (#635), string escapes (#696), and ecosystem build execution
(#618) remain separate work. #629's output-wrap example is not qualified by a
scalar arithmetic fix. The source change must pass exact-head CI before merge,
and remains unreleased until a normal release delivers it.
