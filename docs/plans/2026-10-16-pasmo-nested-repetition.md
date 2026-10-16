# Preserve Pasmo macro boundaries around repetitions (#687)

The default keyword macro collector stops at the first ENDM. Pasmo uses ENDM
for both REPT and macro bodies; a native-valid inner repetition therefore leaves
the true macro closer outside the collected body. The measured PasmoNext bytes
contradict the existing comment that nesting is refused.

1. Probe fresh native PasmoNext fixtures for one/multiple repetition levels,
   parameters, comments/strings and malformed closers, retaining exact sources,
   commands, binary identity and bytes/errors under
   `docs/plans/2026-10-16-pasmo-nested-repetition/`. Add library, formatter and CLI regressions in
   `crates/asm198x/tests/pasmo_nested_repetition.rs`; prove the reported failure against unchanged code.
2. Override only Pasmo's existing `MacroSyntax::collect` seam in
   `crates/asm198x/src/dialects/pasmo.rs`. Preserve nested repetition boundaries
   and source lines without changing the shared default or other dialects.
   Keep the AST/formatter non-expanding; conversion must still reject safely
   where translation is outside its accepted scope. Correct the stale comment.
3. Verify native byte parity, ordinary CLI assembly, formatted-source reassembly,
   error provenance and existing simple macro/repetition behavior. Run relevant
   tests, strict Clippy, formatting and generated docs; retain the actual results.
4. Push a focused PR closing #687. Wait for all exact-head checks and triggered
   workflows before squash merge. Link the result from #687/#641, and clean the
   inspected worktree/build/scratch directories after proving remote retention.

No new dependency, collector abstraction, universal macro grammar or converter
feature is required. The binding macro-expansion decision delegates precisely
this kind of measured difference to the dialect's collector.

## Grounding updates

The non-expanding AST reader has its own line classifier. Pasmo now marks inner
REPT openers through the existing MacroLine enum; the reader counts those only
inside copied macro bodies. No other dialect produces that classification, and
the shared default collector stays unchanged.

The provenance regression also exposed that `place_nodes` remaps only top-level
nodes, leaving repetition/conditional/loop bodies on expanded-text lines. Recurse
through those existing child lists so the newly supported nesting retains its
macro definition/invocation frames. This extends the existing span walk, not the
macro grammar or expansion policy.

A quoted-semicolon fixture fails even outside macros. Keep that separate from
body-boundary handling: use a quoted `rept endm` string to check opener/closer
recognition here, and record the unrelated semicolon behavior as a follow-up.

## Verification

- The initial four behavioral regressions all failed against unchanged code with
  `` `endm` closes a repetition block that was never opened `` at line 6.
  A test-harness compile correction first handled the optional definition span;
  the recorded red run then exercised product behavior, not a compiler failure.
- Fresh PasmoNext v0.1.3 probes cover ten sources: six accepted byte facts and
  four refused forms. Raw commands, streams, bytes and the native executable
  digest live in `crates/asm198x/tests/fixtures/pasmo-nested-repetition.json`.
  Six accepted facts are also appended to the normal Z80 replay corpus.
- All six final regressions pass. They compare both Pasmo front-ends with native
  bytes, preserve unexpanded/idempotent formatting, exercise ordinary CLI output
  in human/JSON modes, retain definition line 2 and invocation line 7 on errors,
  and preserve previous images and source files. Conversion reaches target
  verification and safely refuses untranslated nested syntax without replacing
  its previous source output; broader converter fidelity remains deferred.
- The workspace run passed 1,413 tests, zero failures, 33 intentionally ignored;
  the final targeted run passes all six regressions, including two added CLI
  checks. Strict workspace/all-target Clippy, rustfmt and Ruff pass.
- Generated docs initially flagged ledger/compare/why as stale after adding the
  six verdicts. The existing generator refreshed those three pages. Generated
  documentation and arbitration coverage checks now pass; no coverage stamp or
  accepted shortfall was relaxed.

The first broader fixture used a quoted semicolon, exposing a separate failure
also reproducible without macros. That is kept outside this production fix.
The historical conversion audit remains unchanged: #687 repairs ordinary assembly
and AST boundaries, while the converter continues to reject unsupported target
translation safely. #686's scope/documentation correction remains separate.
