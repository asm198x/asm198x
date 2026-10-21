# Source-query checkpoint after checked #920

#919 closed #810 through merged 75dad590bd8d3e032729bc596f7d3ab3b49a59aa.
#920 is now rebase-merged at 15a2c6dc3492a88f77495e4fa162189e43870c82; its tree
exactly matches qualified source 14295cef80345ac2e38ac16eb2ff20b162595956.
Draft #917 is closed as superseded. Its original source branch at 48e2428d and
rejected audit ref 14faff80 remain retained; no shared history was rewritten.

All 29 required check names passed, including metadata reruns: 32 check
instances and seven runs across five workflow types. A description edit queued
new checks after the first green snapshot, so GitHub safely rejected the first
merge with HTTP 405. The newly triggered checks completed; the full gate was
re-read before the successful rebase merge. All five actual native archives
and source-lock receipts pass 104 cohorts, including every one of the 515 data
cases / 1,853 data command stages and the existing external-binding contract.
Both corruption controls reject. Local artifact-only qualification passes,
and the #810 predecessor fails only the new data cohort. Fresh 4510 verification
passes all 564 bytes and fourteen streams and rejects a changed byte.

The full Lua workspace suite passes 1,643 tests, with 38 ignored. Clippy,
115 Node/setup tests and docs/version/ecosystem checks pass. Actual coverage is
87.74%. All 515 data cases match native, fixing 186 differences; every prior
match across 26 matrices / 4,136 cases survives in debug and release. The
original source-query matrix now matches all 56 cases and visibility all 274.

Final qualification, merge and safe retry evidence is retained on
 audit/ca65-data-context-920 in data-context-v3-qualification-920. The separate
range audit is in docs/plans/2026-10-21-ca65-linked-faraddr on that audit ref.
Cleanup receipts are in data-context-v3-cleanup-920. Fifty completed temporary
paths, including 186,841 inspected files and the completed Rust cache, were
removed after remote retention and lock/process checks. The complete inventory
is compressed; its first uncompressed push drew GitHub's large-file warning,
which was corrected without rewriting pushed history. df observed about
1.6 GiB more available after deletion. All task-owned processes exited.

## Remaining work

Continue #795's approved source-aware design through the remaining numeric
consumers. Its 50-case audit still has 30 differences, in repeat counts,
reserve/fill, align/fill, assertions, numeric output strings, incbin bounds and
include names. Data/conditional contexts now work; reuse those live reader/
projection/expression seams. Preserve name-only queries, source-time types,
provenance, byte-string ownership and lazy reading. Broaden native admission
and ordering controls before changing these consumers. The existing external
cancellation cases also remain explicitly inventoried under #795.

#921 is the independent configured-ca65 faraddr range bug. Its 90-case native
audit has 51 accepts and 39 refusals; qualified #810 and the data candidate both
match 57 and wrongly accept 33. Portable capture is verified. The immutable
issue evidence is on audit/ca65-data-context-920 at 38b55d76. Reuse the existing
approved unsigned Sized policy while retaining the original expression until
layout; write the exact implementation/files/tests plan before code changes.
#918 separately tracks register-token provenance for visibility names. Existing
sizeof gaps remain #798. Do not treat these known gaps as completed parity.

The main-rules API now enforces pull requests and 15 status checks. AGENTS.md's
sentence saying main is unprotected is stale: correct that descriptive sentence
in the next checked source PR, preserving the stronger full CI/native/rebase
convention. PR-description edits trigger metadata workflows; observe those new
runs completing before the final merge request.

## Retained active workspaces

- /Users/stevehill/Projects/198x/Asm198x/asm198x-ca65-text-consumers: sole active
  task worktree, currently on audit/ca65-data-context-920. Reuse for the next
  slice based on checked merged main 15a2c6dc. The original checkout remains
  clean and untouched at local main 47fc5164. Remove this linked worktree only
  after the continuing audit is finished and all unique work is retained.
- /private/tmp/asm198x-data-context-v3-producer: qualified frozen release
  executable b95498886cb76841ff4831c9018eaefb1efc859b3e86bf3fb70f7722bc9f9f43,
  clean committed-source identity and complete 29-matrix / 4,791-case baseline.
  Every native fact is copied into its facts directory with hashes, so future
  source/fact edits cannot change the baseline. Keep until the next checked
  replacement is qualified and retained.
- /private/tmp/asm198x-data-context-v3-local-package: qualified 104-cohort local
  archive and controls. Keep as the next omission-control predecessor; remove
  after a checked replacement and positive recovery inspection.

The old #810 and rejected #917 producers/archives, #920 build/cache/replay/
coverage/raw-candidate directories, range-audit staging and temporary helpers/
logs have been removed. /private/tmp/asm198x-text-consumers-build no longer
exists; recreate it only when needed. A non-incremental task build can bound
future cache growth without changing tests, assertions or project profiles.
Shared native sources at /private/tmp/asm198x-address-size-sources and the
pre-existing shared CI/rebase helpers remain; do not delete unknown siblings.
