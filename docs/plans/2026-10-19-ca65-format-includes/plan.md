# Preserve formatter include operands (#834)

1. Retain native and clean-predecessor outcomes for skipped include/incbin
   operands, computed names, forward offsets, labels and ordinary directives.
   Confirm the formatter failure happens before the no-resolution boundary.
2. In crates/asm198x/src/dialects/ca65/mod.rs, keep include/incbin operands as
   source-only AST nodes in the existing unexpanded formatter mode, after label
   binding but before request/window evaluation. Assembly parsing and include
   loading remain unchanged. No public AST/schema or dependency change.
3. Add crates/asm198x/tests/ca65_format_includes.rs and a retained native fixture.
   Verify idempotence, native bytes and symbol/section shape, including labels
   and anonymous references. Check formatter write/check modes preserve source
   and create no include files; assembly refusals preserve all five outputs.
4. Add a mandatory cohort/omission control in scripts/test-candidate-package.cjs
   and scripts/candidate-package.test.cjs. Verify full Rust/Lua, Node, Clippy,
   fmt/Ruff/docs, actual preceding/current packages and corruption controls.
5. Put only these own task commits onto current main using the approved linear
   workflow. Retain a clean 4510 replay and inspect all exact-head checks,
   five actual host reports/receipts and ten controls before rebase-merging.
   Retain source commits and remove finished owned workspaces and caches.

This is the well-specified no-resolution formatter bug already measured in
#834. The separate #833 formatter design remains unimplemented pending approval.
