# Bind ca65 cheap-local constants to native anchors (#774/#779)

Native accepts `anchor: / @value=7 / .byte @value`; the current constant-name
validator refuses it. Cheap names belong to the current global symbol, not
just the enclosing lexical scope. Native probes also show that ordinary
constant definitions end the previous cheap namespace.

1. Extend the retained native cheap-constant probe into probe.py and
   tests/fixtures/ca65-cheap-constants.json. Measure ordinary/qualified
   definition boundaries, RHS timing, .scope/.proc entry/exit, includes,
   macros, repeats, digit names, forward references and refusal controls.
   Establish failures with bounded tests/ca65_cheap_constants.rs.
2. Reuse existing private cheap_key and ScopeEnv anchors in ca65/mod.rs.
   Admit native cheap constant names, retain their authored name and existing
   Scope::Local metadata, and anchor global constants at their native phase.
   Extend the existing ca65-only scoped_names expression token grammar in
   dialects/mos6502.rs to preserve @name tokens (no new option/trait hook).
   Reuse private expression mapping to bind all cheap operands and name-query
   markers, not only bare values. Pass the current anchor into private
   conditional/repeat folds in ca65/project.rs. Include cheap names in
   Ca65Macros::is_symbol_char in ca65_flat.rs through the existing grammar
   seam, so named parameters cannot rewrite a cheap token's suffix (#779).
   Complete the required scope/form corrections in scope-plan.md before
   merging: newly accepted cheap constants expose wrong bytes (#778/#784). Preserve
   ordinary immutable duplicate checks and frozen AST/diagnostic shapes.
3. Require real-package replay/omission controls. Run native/formatter/debug
   and preservation tests, full Rust/Lua, Clippy, fmt, Ruff, docs, Node and real
   archive controls. Retain complete native/process evidence and clean 4510
   replay. Rebase onto merged #773 before exact-head CI/package qualification.
4. Push, inspect all five actual reports and exact-head checks, squash merge,
   verify producer retention and closure, remove the finished worktree and
   continue remaining native findings.

Follow syntax-stance.md, parse-affecting-directives.md and the existing frozen
shared seams. No dependency, public API or schema change is required. Native
probes decide anchor timing; do not infer it from the old shared helper comments.
