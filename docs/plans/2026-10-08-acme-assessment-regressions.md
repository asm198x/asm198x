# Preserve ACME's non-emitting CPU selection and current-PC conditions

Approved under assessment F5. Keep the existing dialect walk and engine.

## Reproduced faults

1. `!cpu 6502` before `*=$1001` is parsed as an empty `Operation::Bytes`.
   The engine correctly rejects emission before an origin, but a lexical CPU
   selection is not emission. ACME 0.97 accepts this and produces `01 10 a9 01`
   for CBM output followed by `lda #1`.
2. `!if * > $0fff` after `*=$1000` and `nop` fails. The conditional evaluator
   folds with no PC despite the walk retaining it; the comparison scanner also
   treats a trailing location-counter `*` as an incomplete multiplication.

## Changes and verification

- First commit: in `crates/asm198x/src/dialects/acme/evaluate.rs`, retire the
  validated CPU-selection marker after updating lexical target state. Retain
  any label and keep unsupported-CPU diagnostics. Add regressions beside
  the existing CPU-switch tests in `dialects/acme/mod.rs`; compare CLI bytes
  and no-origin/unsupported controls directly with ACME. Do not relax the
  shared engine's origin requirement for data.
- Second commit: pass the walk's current address into the existing conditional
  expression evaluator in `dialects/acme/mod.rs`; recognise comparisons by a
  complete parsed left operand, preserving prefix byte extraction and
  multiplication. Probe PC arithmetic, true/false branches, loops and unknown
  PC conditions against ACME before claiming them. Reuse existing relocation
  state if needed; do not introduce a second evaluator or convergence loop.
- Record red/green output, run ACME-focused library tests, differential tests,
  formatter round trips, then workspace tests and Clippy. No dependency or
  public contract change is planned. Commit locally; do not push yet.

## Narrow expression probe after rollback

The attempted PC extension exposed an independent expression fault. Two
attempts were rolled back to `872800f`; the patch and failing cases are
retained under `/private/tmp/198x-acme-pc-experiment.patch` and
`/private/tmp/198x-acme-probes/`. The lexer calls every `*` a completed value,
even when it is multiplication. Consequently `2 * >value` tokenises `>` as
a comparison. Loose byte extraction is also only admitted at the start of
the arithmetic ladder, not on a binary operator's right side.

Prove this with an ordinary `!byte` probe first. Distinguish the existing PC
atom from multiplication in the existing token enum, and admit loose byte
prefixes through the existing unary entry. Then restore the PC condition
change and run all reference fixtures. This is an internal parser correction;
no new dialect, evaluator, convergence pass or external API is introduced.

## Verified result

The existing walk now supplies its logical PC to condition evaluation, including
nested `!pseudopc` blocks. The shared lexer distinguishes the PC atom from
multiplication; byte prefixes remain valid on the right of multiplication.
A separate false-branch probe exposed an inline parser fault: text after the
first `}` was discarded, including `else`. The existing conditional node now
retains that body and malformed trailing text fails explicitly.

All eleven CLI PC/reference cases agree with ACME 0.97, including the expected
unknown-PC error. All 117 ACME library tests pass. Formatter round trips cover
each live-PC case and both inline branches. The full workspace passes, as do
strict library Clippy and 521 reference-accepted differential snippets plus
the multi-file differential suite. Compressed logs and CLI bytes are retained
in `2026-10-08-acme-assessment-evidence/`. The final inline-tail regression was
added after the workspace run and passes in the focused 117-test run.
