# Pasmo expression arithmetic parity (#634)

The public 0.0.67 CLI succeeds with wrong bytes for oversized divisions;
fresh PasmoNext 0.1.3 produces different results. The existing Pasmo expression
parser masks bitwise/truth results but leaves arithmetic on the shared i64
path. Establish the reference boundaries before choosing the correction.

1. Retain bounded native probes and exact executable identities in
   `2026-10-16-pasmo-arithmetic-634/`: literals, EQU, unary signs, binary
   arithmetic, shifts/comparisons, live PC and invalid operations. Distinguish
   native success, warnings and refusal. Keep #631 address wrapping separate.
2. Add native-backed regressions in
   `crates/asm198x/tests/pasmo_arithmetic.rs` and
   `crates/asm198x/tests/fixtures/pasmo-arithmetic.json`. Demonstrate the current
   source fails the issue cases before production edits. Cover both frontends,
   formatter invariants, CLI reporting and non-Pasmo isolation.
3. Correct the existing Pasmo-only expression lowering in
   `crates/asm198x/src/dialects/z80.rs`, and only other proven necessary seams.
      Native probes show EQU values also wrap, contrary to the existing upper-
   bound test. Update that test and its comment in
   `crates/asm198x/src/dialects/pasmo.rs`; preserve error paths and other
   dialects. Correct the matching false EQU-range comments in `src/dialect.rs`
   and `tests/differential.rs`. Extend the existing Pasmo token mapping for
   the measured shift/logical spellings. Preserve signed index displacement
   interpretation when word-valued expressions reach signed fields. Preserve
   bare entry-symbol identity when the live symbol is already known to fit a
   word, keeping existing Debug198x goldens unchanged. Use existing masked AST
   operations, including the measured 32-bit shift-count behavior; do not
   change the shared evaluator. If measured
   semantics need an API/schema break or a new architecture, stop and agree
   that design before implementing it.
4. Run focused/native regressions, workspace tests, formatting, strict Clippy
   and generated documentation checks. Inspect real outputs and preserve
   source/previous output on invalid CLI requests. Push one bounded PR, require
   every exact-head check/workflow to complete acceptably, then squash merge.
5. Link the actual fix and limits from #634/#640. No release claim until a
   normal release delivers the fix. Preserve remote commits/evidence, remove
   task worktree/build/scratch directories and verify all task processes exit.

No dependency or cross-dialect expression grammar change is planned.
