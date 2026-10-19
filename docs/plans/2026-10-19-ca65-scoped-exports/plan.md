# Bind visibility names in their native scope (#803)

1. Re-consult symbol visibility, scoped environments, syntax, native AST and
   diagnostics decisions. Reproduce the reported local scoped export and root
   fallback using actual ca65/ld65. Retain native export/import/global variants,
   before/after declarations, nested scopes, alias/value definitions, root/local
   collisions, queries, cheap names, invalid qualified names and includes.
2. Compare the explicit preceding #807 executable and write the concrete design
   from native observations. Reuse the current scope environment and final
   outward reference binding, preserving declaration allocation, source-time
   width and reference effects. Do not add object-file linking or a public shape.
3. Correct the smallest existing private reader/projection/validation path, add
   a native fixture and focused CLI tests for bytes/status, formatter/debug
   invariants, included spans and preservation of old outputs on refusal. Keep
   distinct newly found gaps as GitHub issues if this slice cannot own them.
4. Add mandatory actual-package replay and an omission control. Run isolated
   Rust/Lua, Clippy/fmt, Node, Ruff/docs, predecessor/current real archives and
   both corruption controls. Push verified commits and retain clean 4510 replay.
5. Wait for the parent, inspect all exact-head workflows and five actual package
   reports plus ten controls. Squash, verify closure/retention, clean this task's
   workspace/build cache and continue with the next measured issue.

Final implementation files are `crates/asm198x/src/dialects/ca65/mod.rs` and
`ca65/project.rs`; verification adds `tests/ca65_scoped_exports.rs`, its
`tests/fixtures/ca65-scoped-exports.json`, and the existing two candidate package
scripts. The 274 raw controls include explicitly tracked #795/#810 cases;
qualification contains the 222 export/baseline cases this slice owns. The
broader import/global placeholder change was rolled back after native external
resolution probes. Preserve that boundary and prove no new regressions against
the preceding executable across all raw cases.
