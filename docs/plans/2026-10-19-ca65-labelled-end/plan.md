# Bind existing ca65 label forms before .end (#830)

Native `.word finish / finish:.end` emits 02 10 with CODE at $1000.
The selected reader currently recognizes only a bare .end before label parsing;
when it reads an attached label, the remaining .end becomes unsupported syntax.
The selected sweep already truncates unread literal-end tails before expression
or numeric validation, but its head recognizer misses generated identifiers.

1. Re-read syntax, lazy-reading/numeric-admission and scoped-symbol decisions.
   Retain fresh native controls and a hashed canonical predecessor for ordinary,
   cheap, anonymous, generated, absolute, included, macro and repeat label forms.
   Probe reserved instruction/register and F/Z prefix boundaries under 6502 and
   4510, plus duplicate definitions, scopes and invalid captures. Distinguish
   existing whitespace (#848), qualified-definition (#850), register/prefix
   (#853) and unread-structure (#844) gaps explicitly. Do not newly admit invalid
   end labels while fixing ordinary labels.
2. Add a red-first retained fixture and
   `crates/asm198x/tests/ca65_labelled_end.rs`. Check native acceptance/bytes,
   address-kind debug bindings, canonical formatting and idempotence, selected
   versus inactive definitions, and included human/JSON failures preserving
   five outputs and source files.
3. Use existing label binding and label-only selected statements in
   `crates/asm198x/src/dialects/ca65/mod.rs` and `ca65/project.rs`, then set the
   existing end flag. Read the label once. Recognize a literal .end after a
   syntactic generated-label head before evaluating its unread tail. Preserve
   full macro/repeat numeric admission and alias-created-end argument behavior.
   No public schema, dependency, new expression value type or reader architecture.
4. Add a mandatory `ca65_labelled_end` package cohort and omission guard in
   `scripts/test-candidate-package.cjs` and `scripts/candidate-package.test.cjs`.
   Run focused/full Rust+Lua, Node, Clippy, fmt/Ruff/docs, actual native formatting,
   preceding/current archives and corruption controls.
5. Retain a clean 4510 replay, source/tool identities, full bytes and streams.
   Ship only own commits on current main after #859, inspect exact-head CI and
   all five actual reports/receipts and ten controls, then rebase-merge. Verify
   retained work and clean finished owned workspaces, caches and processes.
