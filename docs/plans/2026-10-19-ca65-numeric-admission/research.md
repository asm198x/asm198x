# Native numeric admission

The preceding clean numeric-conversion producer accepts 75 native-invalid
inputs and rejects one native-valid input in the 317 in-scope controls. The
new reader matches all 317: 172 accepted images and 145 refusals. Comparison
streams, exact native tool versions/hashes and the predecessor identity are
retained beside this file.

The shared number converter already has the measured bounds. The remaining
cause was admission: skipped branches, unused macro bodies, zero-repeat
captures, unused macro arguments and token lists could discard tokens before
a number parser saw them. The numeric-only scan runs at existing read/capture
boundaries. It neither interprets skipped directives nor resolves names or
includes. Quoted strings/comments remain opaque. Decimal's 4294967299 endpoint
and the exact hex/suffix/binary token boundaries come from cc65 V2.18
scanner.c (blob 361a817c1383dc0b0aee9c5fa8fcb78f3e66501a,
SHA-256 eea593534c8211287437e6acf9042e74eb5714c08ebe255e16ae5a74096844f8).

Existing source spans and block children identify original delimiter lines,
including a nested elseif sharing its parent's closer. Original source lines
are indexed once per file; a missing source position fails explicitly. Macro
and repeat captures validate their entire token source before execution.
Selected .end then stops before evaluating its operand tail, including inside
a called macro. The existing AST and public contracts are unchanged.

## Boundaries and failures found while verifying

The raw research covers 323 cases. Six linked .setcpu switches belong to the
existing CPU-switching gap #302 and are retained outside the executable numeric
fixture. The 96 new flat CPU controls use the supported 65816 and HuC6280 entry
points instead. Separate original research exclusions are the invalid
multi-character single-quoted literal and two executed-repeat .end cases (#829).

155 native-accepted cases support formatter idempotence, byte-identical
reassembly and stable symbol/section shape. All 172 still require native bytes.
The other 17 retain measured pre-existing formatter gaps: twelve HuC6280 macro
cases (#821), two inactive include operands (#834), and three macro .end cases
(#833). The latter formatter returned success but emitted an unclosed macro;
the full predecessor comparison includes reassembly rather than trusting the
formatter's exit code. These gaps are unchanged by numeric admission.

The first test fails on core_inactive_4294967300 with `left: true`,
`right: false`: the predecessor accepts the oversized skipped token. The
five-output preservation test later exposed a new error-location defect:
`asm198x: input.s:3:8: error: number $100000000 is outside the native lexical range`
(the actual retained stderr includes backticks). Flat include expansion failed
before its normal per-line file stamping. Stamping that boundary now preserves
the included file and exact token column. Both human and JSON modes assert
locations and unchanged binary, symbols, debug and both listing outputs.

The required package cohort is ca65_numeric_admission: 317 assembly commands
plus three formatter/reassembly commands for each of the 155 covered accepted
cases, 782 commands total. Its omission control must fail qualification.

## Local verification

1575 Rust tests pass (38 ignored); all 84 Node tests, Clippy with Lua and
all workspace targets, Rust formatting, Ruff and docs checks pass. The actual
current package passes all 69 cohorts and both corruption controls fail as
required. The clean predecessor package fails only ca65_numeric_admission.
