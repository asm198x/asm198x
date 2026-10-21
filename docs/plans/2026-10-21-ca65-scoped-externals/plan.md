# Scoped external bindings (#810)

The user approved finishing #810 before returning to draft #917 on 2026-10-21.
The agreed scope is in
`../2026-10-20-ca65-source-query-effects/scoped-imports-before-data-design.md`.
Start at merged main f6bf23ef, reusing the existing task worktree. The rejected
data implementation remains on its source and audit remote refs.

1. Reproduce the complete 274-case native visibility matrix against the frozen
   #916 producer. Add `probe.py`/`native.json` here for positional import/global
   queries, collisions, nested fallback, explicit same-file exports, default/zp
   width, transformed expressions and macro/include/lazy controls. Use the
   existing scope-binding capture runner. Retain every source/config/process,
   native byte and executable identity. Keep the eight existing #795 scoped
   data-constancy gaps explicit; conditional source queries already have their
   approved live context.
2. Read `ca65/mod.rs`'s `scope_syms`, `constant_key_at`, `source_value`,
   `scoped_name`, name queries, final `scope_targets` and imported-definition
   validation together. Use existing lexical binding keys and scope machinery
   for declaration metadata. Declarations mask outer bindings in source order;
   conflicting declarations/definitions must refuse at the measured location.
   Do not replay the rejected guard-only patch or add a parallel namespace.
3. In `ca65/project.rs`, `ca65/deferred.rs` and `ca65/link.rs`, preserve source
   constancy and declared address-size independently from final external values.
   Inspect `DefinitionSource`, `study_source`, expression hints, placed
   instruction selection and visibility resolution before edits. Resolve only
   explicit same-file exported bindings through the existing fused linker;
   never use ordinary outer-scope fallback to satisfy an external declaration.
   Pin absolute/zp instruction selection and native data-width refusals even
   when the final exported number is small.
4. After each bounded semantic unit, compare all 274 existing cases and the new
   native matrix; reject any lost previous match. Add the measured contract to
   `tests/fixtures/ca65-scoped-externals.json`, a focused integration harness and
   a mandatory package cohort/omission control. Preserve official debug-reader,
   formatter round-trip and refusal/file ownership invariants.
5. Freeze the implementation before the full workspace suite. Run focused and
   library tests, all-target Clippy with Lua, fmt, Ruff, all Node/setup checks,
   docs/version checks, all retained native matches and fresh 4510 verification.
   Qualify an actual release-feature archive, both corruption controls and the
   #916 predecessor. Push conventional commits and a checked PR; read all 29
   checks plus five actual native reports before rebase merging.
6. Retain final evidence and clean finished task-owned processes/directories.
   Rebase draft #917 only after #810 is checked and merged, then rerun all its
   515 data controls, every retained matrix and full archive qualification.
   Continue #795's 50-case numeric-consumer audit afterwards.

No new dependencies, public schema/API changes, object-file linker, layout-time
string evaluation or lexical fields on Statement. If a significant additional
architectural pattern is needed, write the concrete addition before proceeding.
After two or three failed fixes to one issue, roll back and reduce scope rather
than continuing an unqualified loop.
