# Preserve ca65 lexical references through final binding (#785)

The reader eagerly chooses an already-known outer constant. Native preserves
an unresolved reference in the current lexical scope, which a later local
constant may define; otherwise final binding falls outward. Native address-size
guesses and source-time constancy are separate: a short instruction can later
fail its range check, while an if/res using an unresolved scoped name refuses
at the source point. The retained predecessor examples emit 12 where native
emits 34, and incorrectly accept native source-time errors.

1. Re-consult syntax-stance, scoped-symbol-environment, native AST payload and
   selected-reader decisions. Retain native phase probes for current/outer/root
   names, later definitions, immutable aliases and chains, source-time folds,
   defined/ref/const queries, address-size guesses, scope closure, includes,
   macros/repeats, cycles and undefined names. Verify assumptions before code.
2. Reuse existing scope_ref/scope_targets and ScopeEnv events. Preserve ordinary
   expression references in their current scope; source-time folds see only
   definitions currently known there (or an explicitly named scope). Keep
   source-time name queries and frozen instruction forms on their measured
   paths. Do not rewrite shared AST or engine/dialect/ISA contracts.
3. Retain immutable alias expressions privately where a value is not known at
   definition. Reuse Expr::eval_with as the single evaluator. Resolve only
   affected source-time dependants after a definition; final outward binding
   resolves deferred aliases against complete definitions/labels. Cover cycle,
   undefined-name and range failures. Avoid cloning the full namespace per
   instruction or scanning every alias after every source line. Measure alias
   address-size hints before deciding their representation.
4. Add native/formatter/debug/diagnostic/output-preservation and actual package
   checks with a mandatory omission control. Retain predecessor red evidence.
   Run isolated full Rust/Lua, Clippy, fmt, Ruff, docs, Node and archive controls.
   Measure any hot-path cost against the preceding executable.
5. Rebase onto completed parent fixes, audit a clean unchanged 4510 producer,
   inspect all exact-head checks and five actual reports/ten controls, squash
   through a PR, verify retention/closure, clean and continue the next issue.

The chosen direction preserves native valid programs. Eager outer substitution
emits wrong bytes; rejecting later local definitions would reject valid native
source. This is a correction inside the existing private selected projection
and native linker, with no new dependency or public shape. The user delegated
routine design decisions. Probe results refine the private representation.

Implementation order after the measured phase batch:

- Resolve #791 first in `crates/asm198x/src/dialects/ca65/project.rs` and
  `ca65/mod.rs`: retain private alias expressions/spans, propagate source-time
  constants only to affected dependants and resolve remaining expressions
  against final labels/definition PCs through `Expr::eval_with`. Undefined and
  circular aliases must refuse even when unused. Keep the shared AST unchanged.
- Then correct `Walker::scoped_name` and each scoped source-time fold in
  `ca65/mod.rs` plus conditional/repeat lookup in `ca65/project.rs`. Retain
  placeholders and address-size hints independently from numeric values.
- The linked text-query path in `ca65_flat.rs` currently explicitly refuses
  scoped `.const`; probe and resolve that measured boundary through the live
  scope context instead of flattening names or suppressing the refusal.
- Add `crates/asm198x/tests/ca65_scope_binding.rs`, its retained native fixture,
  and mandatory replay/omission coverage in `scripts/test-candidate-package.cjs`
  and `scripts/candidate-package.test.cjs`. Keep the timed-out native cycle as
  research evidence, outside byte/status test expectations.
