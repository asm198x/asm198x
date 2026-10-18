# ca65 scoped constant formatting (#747, #754, #755, #756, #757, #758)

1. Capture native scope/procedure, nesting, shadowing, alias, include, macro,
   repetition and invalid definition controls in `probe.py` and
   `tests/fixtures/ca65-scoped-constants.json`. Reproduce the formatter refusal
   with bounded CLI tests in `tests/ca65_scoped_constants.rs`.
2. In `ca65/mod.rs`, keep the authored constant name in `Symbol.name` and the
   scope-qualified semantic name in `Symbol.qualified`. Resolve qualified definitions through existing scopes, verifying the complete
   target scope. Reject repeated immutable definitions during the selected
   projection sweep, including unresolved RHS definitions; dead branches remain
   unexamined. Restore the formerly excluded scope formatter
   control in `tests/ca65_4510_bit_branches.rs`.
   Keep implicit `.proc` labels for assembly projection only: formatter source
   already carries the defining directive. Verify procedure idempotence.
   Reuse unique inaccessible scope identities for unnamed `.scope` blocks,
   verified through repetition and macro invocations.
   Preserve external qualified names in formatter parsing, which does not
   resolve includes; verify scope existence during assembly with the loader.
3. Require actual-package replay and omission controls. Rebase onto merged
   #751 before full validation, then run Rust/Lua, Clippy, fmt, Ruff, docs and
   Node checks. Retain a clean unchanged 4510 target replay.
4. Push a PR, inspect all exact-head CI and five actual candidate reports and
   corruption controls, squash merge, clean/reuse campaign resources and
   continue the corpus audit.

Follow-up findings #759 and #760 retain their separate native captures in
`deferred-native.json`; fix their procedure-closer and loop-definition semantics
after this bounded scope/constant slice. They are not qualifying cases here.
