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
