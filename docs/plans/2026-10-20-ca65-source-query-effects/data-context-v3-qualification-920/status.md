# #920 checked and merged

#920 is rebase-merged at 15a2c6dc3492a88f77495e4fa162189e43870c82. Its tree
exactly equals qualified source 14295cef80345ac2e38ac16eb2ff20b162595956.
The old draft #917 is closed as superseded, with its original source branch
and rejected audit ref preserved.

All 29 required check names were satisfied, including metadata reruns:
32 check instances and seven workflow runs across five workflow types. A
PR-description edit queued metadata checks after the initial green snapshot;
GitHub rejected the first merge with HTTP 405. Authoritative state was still
OPEN. The new metadata checks completed, all gates were re-read, and the
rebase merge succeeded. No repository rule or failed check was bypassed.
Actual line coverage is 46,482 / 52,977 (87.73996%).

Every actual native archive report and producer source-lock receipt was read.
All five targets pass 104 cohorts, including all 515 data cases / 1,853 command
stages, plus the existing 1,646-case external contract. Native bytes, JSON,
formatting and refusal streams match. Both corruption controls reject in their
intended ways. The local artifact-only archive passes 104 cohorts and its
#810 predecessor fails only the new data cohort. Fresh 4510 verification passes
all 564 bytes and fourteen streams and rejects a changed byte. Its temporary
build work directory is absent.

The workspace suite passes 1,643 tests (38 ignored), Lua Clippy, 115 Node/setup
tests and docs/version/ecosystem checks. Debug and release retain every prior
native match across 4,136 cases, and all 515 data controls match native.

The continuing frozen baseline now includes 29 matrices / 4,791 cases, with
immutable native facts copied into its producer directory. Two follow-up audits
also agree between debug and release: 50 numeric-consumer cases retain 30 known
differences under #795; the 90-case configured faraddr audit retains 33 false
acceptances under #921. This is not a claim that every audited case matches.

The main-rules API records deletion/non-fast-forward protection, pull requests
and 15 required status checks. AGENTS.md's claim that main is unprotected is
stale; correct that operational sentence in the next checked source PR without
changing the existing stricter checked/rebase/native qualification convention.
