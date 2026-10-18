# Preserve ca65 macro introspection tokens (#770/#771)

Native ca65 V2.18 leaves quoted introspection text unchanged, accepts reserved
keyword casing, keeps macro names case-sensitive, and expands live text names
before answering queries in a generated macro body. The existing substring
replacement emits different bytes or refuses accepted source.

1. Retain bounded ca65/ld65 probes in probe.py and the fixture
   crates/asm198x/tests/fixtures/ca65-macro-introspection.json. Extend the nine
   reproductions with strings, comments, token boundaries, symbol casing,
   arguments, nesting, dead branches and late text aliases. Add native-byte,
   formatter, debug-shape and refusal-preservation tests in
   tests/ca65_macro_introspection.rs; prove the current executable fails.
2. In dialects/macros.rs scan reserved tokens outside strings/comments with
   ASCII-insensitive keyword matching and existing symbol boundaries. Keep
   symbol/parameter matching case-sensitive. For one-layer live expansion,
   defer generated-body definedmacro queries until their next selected read,
   after text substitution. Keep recursive pre-pass consumers unchanged.
   Reuse existing grammar and private functions; no new trait/API/AST shapes.
3. Add the native cohort and omission control to the two candidate scripts.
   Run focused tests, full Rust/Lua, Clippy, fmt, Ruff, docs, Node and a real
   Lua-enabled archive with corruption controls. Retain complete native output,
   negative controls and clean-producer unchanged 4510 replay.
4. Rebase onto merged #768, push, inspect exact-head CI and all five actual
   package reports with controls, squash merge, verify retained commits and
   issue closure, then remove the finished worktree. Continue other findings.

This uses the existing live ca65 reader and macro machinery, following
syntax-stance.md, macro-expansion-framework.md, parse-affecting-directives.md
and the frozen shared seams in roadmap-sequencing.md. User delegated routine
design decisions; no dependency or contract changes are needed.
