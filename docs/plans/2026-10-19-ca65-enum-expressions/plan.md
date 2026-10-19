# Retain ca65 enum expressions (#796)

The current enum reader folds each initializer immediately and drops its
expression. Native retains references in the enum namespace: a later local
member can change an earlier member, while undefined members and value uses still require native validation.
Known-width unused cycles follow the separately measured #800 rules. This task starts with research while #799 qualifies.

1. Re-consult syntax-stance, scoped-symbol-environment, selected-reader and
   native AST payload decisions. Inspect `ca65/mod.rs` record handling and
   `ca65/project.rs`/`ca65/deferred.rs` alias handling. Record native tool identity,
   source/config, bounded process outcomes and bytes for explicit/default enum
   values, forward/local/outer names, source-time folds, queries, labels/PC,
   nesting, negative/boundary values, undefined/cyclic members and size metadata.
2. Compare the actual explicit predecessor with all completed native outcomes;
   retain timeout observations separately. Write the concrete private design
   after the evidence establishes enum cursor and member-size rules. Prefer
   reusing retained immutable expressions and the existing evaluator/queue;
   avoid a numeric-only fallback or a shared/public AST change.
3. Implement only the measured enum correction in `ca65/mod.rs`,
   `ca65/project.rs` and `ca65/deferred.rs` as needed. Keep structs/unions,
   formatter source and debug contracts stable. Add a native fixture and
   `crates/asm198x/tests/ca65_enum_expressions.rs`, included refusal output
   preservation and mandatory actual-package/omission checks in the two
   existing candidate scripts.
4. Verify focused native/formatter/debug tests, full isolated Rust/Lua, Node,
   Clippy/fmt/Ruff/docs, an actual archive and both corruption controls. Measure
   long enum dependencies before accepting a representation. Commit and push
   verified work; audit a clean producer against the unchanged 564-byte 4510
   reference, then qualify every exact-head workflow and five actual packages.
5. Squash through a conventional PR after the parent merges; inspect retained
   receipts and all ten controls, verify merge/closure, remove finished worktrees
   and their disposable caches, and continue the next measured bug.

The user delegated routine decisions and asked the audit/fix campaign to
continue. No dependency, public contract change or new architecture is proposed.
If the probes require one, stop implementation at that boundary and explain it.

The concrete design is in design.md. The source-validation prerequisite merged in #804. Enum execution uses that
unchanged parent code and will incorporate its merged history before
qualification.

The final reviewed fixture has 136 completed native controls (64 accepted,
72 refused). Four native timeouts are retained separately. The new package
cohort is mandatory and executes 328 commands; the required total is 59.
