# Stop executed repeats at .end (#829)

1. Re-consult source-reading and conditional/repetition decisions. Retain the
   eleven existing native observations and add end/include/capture boundaries.
   Reproduce the unclosed-repeat error and bound the billion-count control.
2. In crates/asm198x/src/dialects/ca65/project.rs, stop the repeat loop as soon
   as the selected reader ends. Restore counters/registers/capture depth as
   usual, preserve conditional refusal and unopened deferred character input,
   and remove the erroneous executed-repeat unclosed-block diagnostic.
3. Add crates/asm198x/tests/ca65_repeat_end.rs and a retained native fixture. Require native
   acceptance/bytes, valid formatter invariants, bounded huge counts and
   five-output/source-location preservation. Track existing formatter #833
   separately rather than treating successful formatting as a round trip.
4. Update scripts/test-candidate-package.cjs and scripts/candidate-package.test.cjs with a mandatory cohort and omission control. Verify the
   actual preceding/current archives and both corruption controls, full Rust
   with Lua, Clippy/fmt, Node, Ruff and docs.
5. Push the source, retain a clean unchanged 4510 replay, and qualify the exact
   final head on all five hosts. Merge only after #788 lands and all checks,
   receipts and ten corruption controls pass. Retain commits and clean owned
   worktrees, caches and processes. No dependency or public-contract change.

This is the small, measured repeat-termination bug already recorded in #829;
source capture and the approved numeric-admission policy stay as implemented.
