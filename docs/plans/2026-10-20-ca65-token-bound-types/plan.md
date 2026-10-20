# Reject string-valued token-list selector bounds (#884)

Native rejects a quoted bound, while Scope::number currently evaluates its
decoded text as an expression. The 623-case #872 audit positively reproduces
seven false acceptances. Keep this ca65 selector check separate from quoted
token-list admission and from other dialects' numeric-argument rules.

1. Capture dedicated native literal/nested quoted bounds on .left/.right and
   both .mid positions, alongside numeric-expression/constant/character and
   large/negative controls. Include selected/skipped, captured/unused macro,
   repeat and included text in this directory's probe.py/native.json. Retain
   a comparison against clean producer c478532a4f4fee2fd608a594f8e0f33a125b01a4.
2. Add a red-first native byte/debug/formatter fixture and regression in
   crates/asm198x/tests/ca65_token_bound_types.rs and
   crates/asm198x/tests/fixtures/ca65-token-bound-types.json. Include human/JSON
   include/macro error spans and preservation of all prior outputs/source.
3. In crates/asm198x/src/dialects/ca65_flat.rs, require Arg::Bare at the three
   selector-bound reads before calling the unchanged Scope::number. A private
   helper can share the ca65 check; keep existing value/bounds behavior and
   shared TextSyntax/Arg/expression representations unchanged. No new dependency,
   API, scope state or architecture is needed for this measured type check.
4. Require the native cohort and omission guard in
   scripts/test-candidate-package.cjs and scripts/candidate-package.test.cjs.
   Confirm the full 623-case comparison removes exactly the seven old wrong
   acceptances and introduces no other difference.
5. Freeze the focused CLI before full Rust+Lua; run Node, native formatting,
   Clippy/fmt/Ruff/docs, prior/current actual archives and corruption controls.
   Commit/push, verify a clean matching producer and full 4510 replay. Import
   only this slice on main after #883, inspect exact-head CI, five actual package
   receipts and all ten controls before checked rebase merge and workspace reuse.
