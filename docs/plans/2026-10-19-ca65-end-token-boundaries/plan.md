# Recognize native .end tokens and labels (#830/#842)

1. Retain the fourteen native token-boundary controls and clean predecessor
   refusals. Extend them with the labelled .end controls from #830, generated
   labels, scopes, includes and captured token streams before changing code.
   Re-consult source-reading, label and diagnostic decisions.
2. In crates/asm198x/src/dialects/ca65/mod.rs, recognize the native .end keyword
   at token boundaries, preserving distinct longer words. Bind an optional label
   at the authored address before ending the selected reader. Reuse existing
   empty-label placement and source spans, without a public AST/schema change.
3. In crates/asm198x/src/dialects/ca65/project.rs, use the same boundary for the
   unread operand tail before text evaluation/numeric admission. Original macro
   and repeat captures must still validate their complete token source first.
   Keep include input priority, conditional errors and physical offsets.
4. Add crates/asm198x/tests/ca65_end_tokens.rs and a retained native fixture.
   Compare acceptance/bytes/labels/debug positions; test longer-word refusals,
   dynamic label tails, included diagnostics and five-output preservation. Pin
   formatter coverage separately against the existing #833 source-loss gap.
5. Add a mandatory cohort/omission control to scripts/test-candidate-package.cjs
   and scripts/candidate-package.test.cjs. Verify full Rust/Lua, Node, Clippy,
   fmt/Ruff/docs, actual preceding/current archives and both corruption controls.
6. Move only these own commits onto current main, without merge commits or
   force-pushing published history. Retain a clean 4510 replay, inspect all current
   exact-head checks and five actual packages/receipts plus ten controls, then
   rebase-merge, retain source and clean owned workspaces/caches/processes.

The initial native audit finds eleven accepted forms and three longer-word
refusals; ten accepted forms fail in Asm. The pre-admission producer also rejects
an attached comma tail, so this is an existing directive-recognition gap rather
than a new numeric-validation regression. The formatter design in #833 remains
unimplemented pending approval; this plan fixes selected assembly recognition.
