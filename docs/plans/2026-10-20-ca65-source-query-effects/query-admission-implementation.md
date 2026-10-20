# Selected conditional admission

The conditional reader scopes the original expression and answers name-only
queries before folding. Its ordinary symbol dependencies now remain in private
Deferred metadata with the complete conditional node span. Final assembly adds
those original keys to the same scope-target resolution used by statements and
aliases, then rejects a target absent from the selected final definitions.
Explicit root lookup remains exact; ordinary scoped lookup retains outward
fallback. No final numeric address participates in source-time classification.

The 54-case native audit retains 35 successes and 19 refusals. This slice fixes
12 wrong acceptances: missing conditional names, cancelled dependencies, zero
products and explicit root names, each directly, included and macro-expanded.
The predecessor fails all twelve in the supported 51-case fixture. The corrected
CLI matches those 51 cases. Three logical `.const(0&&MISSING)` refusals remain
explicitly excluded under #795 because text expansion still erases that read
before the conditional reader. They require the approved ordered live context.

The earlier classifier audit's explicit outer-alias refusal also now matches.
Its permanent fixture includes all 314 native cases, with no exclusions. All
2,194 earlier replay cases retain every previous native match. The new fixture
also checks output preservation, real JSON/file bytes, the official debug reader,
formatter idempotence and reassembly, and symbols/sections after formatting.
The twelve newly rejected cases check include paths, source lines and macro
frames. The required package cohort is `ca65_query_admission` (51 cases, 156
commands); `ca65_source_constancy` now has 314 cases and 1,229 commands.

The retained predecessor archive should fail both cohorts: it misses the new
admission cases and still accepts the formerly excluded classifier case. An
omitted query cohort separately fails the required-cohort unit guard.

Remaining #795 work is the approved source-aware parser/text context: consume
ordinary names, `.const` and numeric string arguments in expression order,
including names discarded by short-circuiting. Reuse these final admission
requirements at that live seam; do not mark a whole operand tail in advance.
