# Retain deferred ca65 immutable aliases (#791)

An unresolved Item::Equ currently retains only its name in equ_definitions and
then drops its expression. Native forward label/PC aliases succeed; unused
undefined and circular aliases refuse. Root-level probes reproduce this
independently of #785's lexical binding. Correct this private projection seam
before changing scoped-name lookup. The user delegated routine decisions.

1. Retain native ca65/ld65 cases in tests/fixtures/ca65-deferred-aliases.json:
   forward constants/labels, PC under placement/org/segments, chained aliases,
   source-time updates, cycles/undefined unused definitions, known/backward,
   visibility, formatter and scoped controls. Preserve the predecessor red.
2. In ca65/project.rs retain unresolved expressions with their original span
   and definition position. Propagate newly known numeric values through only
   affected dependencies, into the existing reader/projection environments.
   Do not clone the entire namespace or repeatedly rescan every alias.
3. In ca65/mod.rs resolve remaining aliases after final scope binding and
   label placement through Expr::eval_with. Capture definition PCs through
   private zero-size placed statements, and report errors at the definition.
   Use an iterative dependency queue so long chains/cycles cannot recurse.
   Preserve the shared AST/engine/API, source-time instruction forms, debug
   sidecars and symbol visibility. No new dependency or architectural seam.
4. Add tests/ca65_deferred_aliases.rs with native status/bytes, formatter
   idempotence/reassembly/debug checks and included refusal preservation.
   Add mandatory package replay and omission controls in both candidate
   scripts. Verify isolated Rust/Lua, Node, Clippy, fmt, Ruff, generated docs,
   actual local archive and both negative controls; measure long alias chains.
5. After parents merge, preserve tested source subtrees, replay the clean
   unchanged 4510 producer, publish a PR and inspect all exact-head checks plus
   five actual platform reports and ten controls. Squash, prove retained
   producer/closure, clean the task workspace and continue #785.

Re-consult syntax-stance, scoped-symbol-environment, native AST payload,
parse-affecting-directives, symbol-visibility-in-a-fused-assembler and the
core contract freeze. Reuse existing scope_ref/scope_targets and evaluator;
no shared Statement lexical fields or additional scope-mangling design.
