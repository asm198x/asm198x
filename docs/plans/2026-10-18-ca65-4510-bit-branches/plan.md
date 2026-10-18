# ca65 4510 bit branches (#745)

Use the existing ca65 native payload and published ISA forms. No shared ISA,
public AST, dependency or other dialect changes.

1. Capture native ca65/ld65 controls in `probe.py` and
   `crates/asm198x/tests/fixtures/ca65-4510-bit-branches.json`: all sixteen
   mnemonics, PC expressions, width/range limits, labels, scopes, selectors,
   includes and malformed operands. Prove the current parser fails.
2. In `ca65/mod.rs`, `project.rs` and `link.rs`, retain and map both expressions
   and resolve the existing `zeropage,relative` form with native PC semantics.
   Add differential, formatter and CLI preservation tests in
   `tests/ca65_4510_bit_branches.rs`.
3. Require the fixture in `scripts/test-candidate-package.cjs` and prove its
   omission fails qualification in `scripts/candidate-package.test.cjs`.
   Run Rust/Lua workspace, Clippy, fmt, docs and Node checks.
4. Commit the clean producer, replay pinned unchanged 4510 target, retain
   complete output/process evidence and verify full bytes or file next boundary.
5. Push, open PR, inspect all exact-head checks and five actual candidate
   reports (including negative controls), squash merge, and clean task resources.
