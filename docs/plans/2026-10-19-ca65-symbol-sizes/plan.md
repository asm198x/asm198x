# Retain native label and procedure sizes (#798)

1. Re-consult scope, native AST, layout, syntax and artifact-contract decisions.
   Retain bounded native ca65/ld65 controls for ordinary/cheap/anonymous labels,
   same-line and empty declarations, forward/outer/local size lookup, procedure
   and scope boundaries, aliases, alignment/reservations, segment switches,
   org/reloc, includes/macros/repeats and source-reference effects. Compare the
   explicit preceding scoped-export executable before changing code.
2. Write the concrete design from the observed native lifetime and meaning of
   each size. Inspect `ca65/mod.rs` and `ca65/project.rs`'s live source offsets,
   scope frames, private size properties and debug output. Reuse those paths;
   preserve public AST/artifact shapes and ordinary value binding.
3. Implement the smallest private correction and add
   `tests/ca65_symbol_sizes.rs` plus `tests/fixtures/ca65-symbol-sizes.json`.
   Prove native bytes/status, source-time availability, formatter/debug/area
   invariants and included refusal preservation of all five old outputs.
4. Add a required cohort and omission control to the two existing candidate
   package scripts. Verify isolated Rust/Lua, Clippy, fmt, Node, Ruff and docs;
   inspect actual preceding/current archives and both corruption controls.
   Push verified task commits and retain a clean unchanged 4510 producer replay.
5. Wait for the parent and every exact-head check/workflow; inspect all five
   actual package reports, receipts and ten corruption controls. Squash with a
   conventional subject, verify closure/retention, clean the finished task
   workspace/cache and continue the next measured issue.
