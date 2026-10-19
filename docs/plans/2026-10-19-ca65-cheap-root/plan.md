# Require native cheap-local anchors at selected binding/lookup (#875/#878)

Native rejects a cheap label before any global symbol exists. The current label
splitter and query resolver manufacture an empty-anchor key; constants already
check for an absent anchor. Use the existing shared ScopeEnv anchor as the sole
state, preserving the native definition events settled by #774/#778.

1. Re-consult scoped-symbol and eager-structure/lazy-meaning decisions. Retain
   native root/anchored label, constant, expression/query and conditional controls;
   compare ordinary/generated names, digit/dot token boundaries, anonymous labels,
   named/unnamed scopes/procs, origin/segment changes, macros/repeats and includes.
   Separate missing-anchor failures from undefined-value outcomes and other
   spelling/token gaps; file each confirmed independent bug with evidence.
2. Add red-first `crates/asm198x/tests/ca65_cheap_root.rs` and a native fixture.
   Guard selected resolution/binding at existing seams in
   `crates/asm198x/src/dialects/ca65/mod.rs` and, if required,
   `ca65/project.rs`; do not add a second scope map, new name mangling or
   Statement fields. Keep canonical formatting syntax-only and source-preserving.
3. Add required cohort/omission checks in `scripts/test-candidate-package.cjs`
   and `scripts/candidate-package.test.cjs`. Once fixed, restore #875's two
   separately accounted controls to `fixtures/ca65-ident-names.json`, update its
   Rust/cohort count and native formatter verification, and remove the exclusions.
4. Verify focused/full Rust, human/JSON include/macro provenance and preserved
   outputs, actual native formatter outcomes, Node, Clippy/fmt/Ruff/docs, previous
   and current real archives plus corruption controls. Freeze a clean producer
   and retain complete clean 4510 native bytes/process streams.
5. After #877 merges, ship isolated commits on current main. Inspect exact-head
   CI, five actual package reports/receipts and all ten controls before rebase
   merge; clean or reuse owned workspaces after source/evidence retention checks.

The label token correction is #878. Fourteen anchored undefined .ifconst
controls are accounted separately under #879; the new native fixture has 315
of 329 cases. The previous identifier cohort regains its two #875 controls.
