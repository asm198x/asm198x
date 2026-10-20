# Literal-boundary verification

The source fixes 60 wrong native acceptances on linked ca65 and 94 across the
flat 65816/HuC6280 readers. Every preceding native match remains a match in the
complete 354/708-case audits and the nine wider byte/token/lifetime/text audits.
The remaining differences are retained under #835/#895 and the existing quoted,
scoped-query, macro-argument and textual-consumer issues.

The required cohort covers 768 native cases: 477 accepted and 291 refused,
with 2,016 CLI commands. It checks every accepted image, official Debug198x
parsing and four-output preservation on refusal. Canonical/idempotent formatting
and reassembly cover 416 accepted cases; the other 61 retain the explicit
HuC6280 macro formatter gap #821. Complete coverage accounting retains all
1,062 native cases: 30 exclusions under #835 and 264 under #895.

The full workspace/Lua suite passes 1,620 tests, with 38 ignored. All 105 Node
checks pass. Clippy over every target with Lua, rustfmt, Ruff and generated docs
checks pass. The preceding #894 archive fails exactly the new cohort while all
89 earlier cohorts pass. Logs, native outcomes and the red package report are
retained here. Clean producer, fresh 4510 replay, local archive controls and
actual five-host qualification remain required before merge.

Both original byte-string worktrees have been removed. All unique audit commits
remain on the verified remote audit branch; every duplicated prototype file was
checked against retained shipping history before deletion. The cleanup manifest
and #894 merge/package summaries are retained here.
