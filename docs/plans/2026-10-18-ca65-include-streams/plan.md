# ca65 include streams (#765, #766)

1. Capture native repeat/macro include order, LIFO file order, scope and text
   state, nested stream boundaries, ordinary/conditional includes, zero/dead
   branches, relative search, binary includes, repeated files, labels, and
   diagnostics. Reproduce incorrect bytes and acceptance with bounded CLI tests.
2. Refine the existing selected projection sweep: open selected includes where
   their directive is read, retaining source resolution and immediate failures;
   defer their contents while a repeat or macro token stream remains active.
   Consume pending files in LIFO order at the outer token-stream boundary.
   Use existing macro expansion frames to delimit invocations and the existing
   repeat recursion to delimit loops. Keep the authored AST and all public
   contracts unchanged. Document this measured qualification to the existing
   parse-affecting-directives decision; no whole-file preprocessor is added.
3. Track pending file search order separately from true recursive ancestry:
   repeated pending siblings are valid, while recursive cycles/depth still
   fail. Preserve each loaded file's boundaries, diagnostics and pending-file
   watermark while processing nested token streams. Binary includes stay
   immediate. Add private shared loader support only as needed, retaining its
   existing ordinary-include entry point and guards.
4. Run focused native/formatter/debug/output-preservation tests, a local real
   Lua-enabled archive preflight, Rust/Lua, Clippy, fmt, Ruff, docs and Node.
   Align with the merged repeat-token slice before the clean 4510 replay and
   exact-head five-host package qualification. Retain the failing controls.
5. Push the PR, inspect every workflow and all five reports/negative controls,
   squash merge, remove finished worktrees, and continue the remaining findings.

Track prior text-symbol expansion in captured directive operands separately as
#768, retaining its native refusal controls in `deferred-native.json`. This slice
checks late immutable definitions as the include-content state control.

The first implementation exposed missing macro origins on conditional/repeat
header nodes in the multi-file cursor. Place those existing spans from the same
origin map as ordinary lines, so stream boundaries and diagnostics retain the
whole invocation across nested blocks. No span shape changes.

Exact files:
- `crates/asm198x/src/dialects/ca65/project.rs`: selected token boundaries,
  pending file consumption, search order and true recursive ancestry.
- `crates/asm198x/src/dialects/ca65_flat.rs`: loader entry retaining existing
  ordinary guards, and block-head origin placement.
- `decisions/parse-affecting-directives.md`: measured input-priority refinement.
- `crates/asm198x/tests/ca65_include_streams.rs` and
  `tests/fixtures/ca65-include-streams.json`: differential/formatter/debug and
  human/JSON preservation/cycle checks.
- `scripts/test-candidate-package.cjs` and `scripts/candidate-package.test.cjs`:
  actual archive replay and mandatory-cohort omission control.
- This plan folder: native probe, deferred captures, logs, tree identities,
  replay verifier and full unchanged 4510 evidence.
