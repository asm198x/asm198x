# Formatter .end source preservation

1. Agree the design in design.md. Retain native root/macro/repeat/conditional,
   quote/comment, unread-tail and newline controls before changing behaviour.
2. Implement only the agreed scope in crates/asm198x/src/dialects/ca65/mod.rs,
   using existing AST Verbatim source nodes and the unexpanded formatter path.
   Add crates/asm198x/tests/ca65_format_end.rs and its retained native fixture.
3. Verify source preservation, idempotence and native byte-equivalent assembly,
   including zero repeats, unused/called macros and ignored malformed tails.
   Preserve assembly diagnostics and outputs for native-invalid capture tokens.
4. Update scripts/test-candidate-package.cjs and scripts/candidate-package.test.cjs
   with a required cohort and omission control. Run full Rust/Lua, Node, Clippy,
   fmt/Ruff/docs, actual preceding/current archives and corruption controls.
5. After #788/#829 land, move the own task commits onto current main without
   merge commits or force-pushing shared history. Retain a clean 4510 replay,
   inspect all current exact-head checks, five actual host reports/receipts and
   ten controls, then rebase-merge, retain commits and clean owned workspaces.
