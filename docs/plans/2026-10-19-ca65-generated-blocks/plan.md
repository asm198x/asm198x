# Recognize generated labels on selected block heads (#861)

Native `.ident("mark"):.if 1` and `.ident("mark"):.repeat 2` bind the label and
execute their bodies. The structural keyword classifier currently strips only
static label heads, so those deferred lines reach the ordinary operation parser
and fail as unsupported .if/.repeat. The selected block reader already evaluates
text and binds a label before folding the condition/count.

1. Re-read eager-structure/lazy-meaning, numeric admission, text-layer and scoped
   symbol decisions. Retain native controls for selected/inactive/nested heads,
   all supported conditional forms, zero/nonzero repeats/counters, generated
   expressions, constants, CPU-dependent reserved names, includes/macros and
   malformed heads. Probe labelled closers separately; do not infer support.
2. Add a red-first fixture/test in
   `crates/asm198x/tests/ca65_generated_blocks.rs`. Assert native bytes, address
   labels, selected definition events, formatter round trips, and human/JSON
   output/source/frame preservation. Name existing assembly gaps explicitly.
3. In `crates/asm198x/src/dialects/ca65/mod.rs`, let structural opening-keyword
   classification use the existing private generated-label syntax helper without
   evaluating the argument. Continue semantic evaluation/binding through
   `ca65/project.rs::read_block_head`. Preserve lazy text/includes, full capture
   numeric checks and CPU-dependent name policy; refuse native-invalid heads
   rather than newly admitting reserved names. Share the existing private
   end-label token policy when validating selected block heads; instruction
   names remain refused even when manufactured by .ident. Keep six labelled
   closer cases tracked separately and unchanged in this opening-head slice. No public API/schema or dependency.
4. Require a `ca65_generated_blocks` cohort and omission guard in
   `scripts/test-candidate-package.cjs` and `scripts/candidate-package.test.cjs`.
   Run focused/full Rust+Lua, Node, Clippy, fmt/Ruff/docs, actual native formatting,
   preceding/current archives and corruption controls.
5. Retain a clean 4510 replay with full bytes/streams. Ship own task commits on
   current main after #864, inspect exact-head checks, all five actual host
   reports/receipts and ten controls, rebase-merge, then clean owned workspaces,
   caches and processes after verifying retained source/evidence.
