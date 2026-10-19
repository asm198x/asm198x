# Refuse native-invalid three-apostrophe character tokens (#855)

The shared Z80 tokenizer admits any three-character quote shape, including
three apostrophes. Native PasmoNext v0.1.3 rejects that input. Six native controls
retain packed/empty/apostrophe refusals plus ordinary characters, AF' and
numeric-apostrophe boundaries; the clean predecessor falsely accepts only the
three-apostrophe case. This is a syntax bug, independent of SjASMPlus #737.

1. Add red-first native fixture and tests at
   crates/asm198x/tests/pasmo_character_literals.rs and
   crates/asm198x/tests/fixtures/pasmo-character-literals.json. Verify both Pasmo
   entry points, formatter bytes/symbols and five-output/include diagnostics.
2. Correct only the existing three-character branch in
   crates/asm198x/src/dialects/z80.rs so its middle character cannot be an
   unescaped delimiter. Do not change Pasmo quoting, numeric separators, or AF'.
3. Add the mandatory package cohort/omission control in
   scripts/test-candidate-package.cjs and scripts/candidate-package.test.cjs.
   Run focused/full Rust+Lua, Node, Clippy, fmt/Ruff/docs and actual archive
   controls. Retain the clean producer and raw outcomes before continuing #737.
4. Ship the focused commit with the related quote batch through exact-head
   checked rebase PR; inspect five actual reports/receipts and ten corruption
   controls, verify remote retention, then clean task-owned workspaces/caches.

No dependency, public API/schema, layout rule or architectural change.
