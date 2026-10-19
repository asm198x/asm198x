# Recognize native .end token boundaries (#842)

This slice fixes the literal directive token only. #830 label binding is a
separate follow-up; its extended native research is retained here. #844 covers
unclosed blocks on later unread lines, and #833 formatter preservation remains
pending approval. No general label grammar or structural-reader rewrite belongs
in this change.

1. Retain the fourteen initial and twenty-five expanded root/capture controls.
   Verify default identifier boundaries and longer-word refusals before changing
   crates/asm198x/src/dialects/ca65/mod.rs.
2. Add one private token-length helper in that module, reused by the reader and
   structural keyword classifier. A literal .end must win before a colon can be
   mistaken for a label/conditional head. Keep .endmacro, .end2 and .end_thing
   distinct. No public AST/schema or dependency change.
3. Use the same boundary in crates/asm198x/src/dialects/ca65/project.rs before
   text evaluation/numeric admission of selected character source. Original
   macro/repeat captures still validate their entire source before execution.
4. Add crates/asm198x/tests/ca65_end_tokens.rs and retained native fixture.
   Compare acceptance/bytes, includes, macro/repeat streams, inactive/captured
   numeric refusals and five-output preservation. Keep formatter #833 separate.
5. Add a mandatory cohort/omission control in scripts/test-candidate-package.cjs
   and scripts/candidate-package.test.cjs; verify full Rust/Lua, Node, Clippy,
   fmt/Ruff/docs, actual preceding/current archives and corruption controls.
6. Put only this slice's own commits onto current main; retain a clean 4510
   replay, inspect current exact-head checks and five actual packages/receipts
   plus ten controls, then rebase-merge and clean owned workspaces/caches.

The initial native audit has eleven accepted forms and three longer-word
refusals; ten accepted forms fail in Asm. Pre-admission producer 86b5608 also
rejects an attached comma tail: this is an existing recognition gap, rather
than a numeric-admission regression. Broader label/structure cases remain
visible in native-extended.json without being claimed as fixed here.
