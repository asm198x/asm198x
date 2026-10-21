# Source-query checkpoint after checked #810

#810 is closed through rebase-merged #919 at
75dad590bd8d3e032729bc596f7d3ab3b49a59aa. Its tree exactly equals qualified
source 9e448f2c496c5c6641164ddb8097f34f28cf4bf9. All 29 exact-head checks and
five workflow gates are satisfied. All five actual native archives pass 103
cohorts, including 5,441 commands for 1,646 external cases; both corruption
controls reject as intended. Full workspace tests: 1,642 passed, 38 ignored.
Coverage: 87.72%. Local archive, predecessor omission control and fresh
564-byte / fourteen-stream 4510 replay pass their intended checks.

Complete native/local/CI/merge evidence is retained on
 audit/ca65-scoped-externals-919 in
 docs/plans/2026-10-21-ca65-scoped-externals/qualification-919.
Cleanup receipts are in cleanup-919. Fifty-five completed temporary paths
(804 files, about 535 MB logical size) were inspected, retained and removed.
All task-owned build/test/archive/monitor processes exited. The original
checkout remains clean and untouched at local main 47fc5164.

## Resume #795 data queries

Draft #917 remains deliberately unmerged at source
48e2428d565e3c7a010ae50963198b47b76e9650. Its 515 native data cases passed,
but a #810 import violation was masked by the previous constancy refusal.
The rejected guard-only attempt and four failed native reports are retained
on audit/ca65-data-context-917 at 14faff80. Do not repeat the guard patch or
weaken any test. #810 now supplies the complete coupled binding contract.

Start a new branch at the checked #919 merged base. Reapply only the implementation changes from
ce3955b3 and 48e2428d; preserve the original draft branch and audit refs.
The concrete next plan is retained as the gzip file
qualification-919/asm198x-external-bindings-next-data-plan.md.gz on the #919
audit ref. Write it into the source-query plans folder before code changes.
Use the already approved source-aware context design and existing reader/scope
seams; keep all new external checks and the pure-text admission guard wherever
live ordered evaluation has not replaced it.

The frozen #810 executable still differs on 186 of 515 data cases. The next
numeric-consumer audit still differs on 30 of 50 native accepts (20 lazy
controls pass). Both new baselines and native facts are retained in #919's
qualification directory. The full retained replay now contains 26 matrices /
4,136 cases. #918 separately tracks register-token provenance for visibility
names, preserving valid .ident-generated names.

## Retained workspaces

- /Users/stevehill/Projects/198x/Asm198x/asm198x-ca65-text-consumers: sole active
  task worktree, now on fix/ca65-ordered-data-queries-v3, based at checked #919.
  Remove with git worktree remove only after every unique change and commit
  is retained and the continuing audit is finished.
- /private/tmp/asm198x-text-consumers-build: shared debug cache for continuing
  tests. Its CLI now contains the uncommitted ordered-data candidate under verification. Do not rebuild while a
  full suite runs; remove at the end of the continuing task.
- /private/tmp/asm198x-external-bindings-final-producer: qualified #810 frozen
  release executable, identity and 26-matrix replay recipe. Executable SHA is
  6481805788612c6da60b2db2bd38c6e5693d44ebf35a6f6be1696460f27c0ea0.
  Keep until the next checked replacement is qualified and retained.
- /private/tmp/asm198x-external-bindings-final-local-package: qualified local
  artifact-only #810 archive and controls; keep for the next omission control.
  Remove after a checked replacement and positive recovery inspection.
- /private/tmp/asm198x-data-context-v2-producer: rejected #917 source 48e2428d
  frozen executable (SHA e68edf1e9d4dd73044df9ff21d56e422dacca306e155f33e874a332140c8d28f).
  Keep until the data replacement is checked, then inspect/retain/remove it.

The old #916 producer/archive and #810's isolated build cache, raw candidate
reports, coverage download, fresh audit directory, two debug replays, staging
and temporary helpers/logs have been removed after remote retention checks.
Shared native sources at /private/tmp/asm198x-address-size-sources and the
pre-existing shared CI/rebase helpers remain; do not delete unknown siblings.


## Active ordered-data implementation

The plan/baseline commit 71d38b7e is pushed on
fix/ca65-ordered-data-queries-v3. Only the implementation changes from ce3955b3
and 48e2428d were reapplied on the checked merged base; the rejected receipts
remain on their audit refs. The four mechanical overlaps retained both fixtures,
both mandatory cohorts and the existing pure external admission guard. There
are no unresolved Git conflicts. Source remains uncommitted pending full tests.

All 515 data controls now match native, fixing 186 differences. All 4,136 old
cases retain their previous native matches. Focused tests pass, including all
1,646 external cases and the old import failure. Clippy and 115 Node/setup tests
pass; the full workspace suite passes 1,643 tests (38 ignored). Shared CLI must remain frozen.

Additional active task directories:

- /private/tmp/asm198x-data-context-v3-replay: 26-matrix debug replay and summary;
  retain with final evidence, then remove after remote retention checks.
- /private/tmp/asm198x-data-context-v3-release-build: isolated release cache;
  freeze its executable and verify against clean committed source before final
  qualification; remove after a retained checked replacement.

Files prefixed asm198x-data-context-v3- in /private/tmp belong to this slice:
source patch, apply log, plan commit output, replay helper and verification logs.
Inspect and retain them before cleanup. No new worktree was created.

- /private/tmp/asm198x-data-context-v3-producer: frozen release executable and
  native replay; verify its identity against clean committed source before
  actual qualification, then retain until the next checked replacement.
- /private/tmp/asm198x-data-context-v3-local-package: prepared local archive,
  currently unqualified; keep for the imminent positive and negative controls.
- /private/tmp/asm198x-ca65-linked-faraddr-audit: independent 90-case native
  range audit, comparing both qualified #810 and the data candidate. Both have
  the same 33 false acceptances; no matching open issue was found. Retain this
  audit separately and file its evidence after the current source commit.
- /private/tmp/asm198x-audit-data-context-v3-candidates.py: actual native report
  inspector adapted for the required data cohort; retain before cleanup.
