# 0.0.70 native package checkpoint

All five native candidate/public archive pairs pass the frozen 30-case cohort:
300 passing cases across macOS ARM/Intel, Linux ARM/x86-64 and Windows x86-64.
The release workflow completed all five qualifications before hosting; the
separate published-download run verified the same archive, executable and
producer-receipt identities. Wrong-byte and corrupt-archive controls fail in
all ten target/phase combinations.

- Release: [asm198x-v0.0.70](https://github.com/asm198x/asm198x/releases/tag/asm198x-v0.0.70), published 2026-10-16 15:27:41 UTC.
- Source/tag commit: `4d778cd2b424ff2b7b2e93c03e4096ec79e1ccf0`.
- Committed Cargo.lock SHA-256: `db159ba5cfbe63c0c21cbeb00e8629c695daa999d423a9e3ab08792e0058730d`.
- Actual release qualification: [38922798296](https://github.com/asm198x/asm198x/actions/runs/38922798296).
- Actual published downloads: [38924403779](https://github.com/asm198x/asm198x/actions/runs/38924403779).
- Release PR [#712](https://github.com/asm198x/asm198x/pull/712), head `f0667ec277d3ca531be195856055abbe7d824a6f`: all 36 checks and six triggered workflows completed successfully or were explicitly skipped/neutral before merging.
- Fix PRs [#710](https://github.com/asm198x/asm198x/pull/710), [#711](https://github.com/asm198x/asm198x/pull/711) and [#713](https://github.com/asm198x/asm198x/pull/713): each exact head had 32 completed checks and five completed workflows before merging.

The package cases add wrapped Pasmo images and SjASMPlus word operators, and
replace the bounded escape-refusal control with native decoding plus malformed
input preservation. The actual older public 0.0.69 archive passes the other
27 current cases and fails exactly those three corrections; its historical
28-case qualification remains unchanged.

The source-lock receipts are unsigned producer evidence. They bind the observed
archive/executable to the producer's committed lock and checkout; their compiled
dependency graph remains explicitly unknown.

Run the frozen checks without importing the evolving package gate:

```sh
node docs/plans/2026-10-16-release-0070-checkpoint/verify.cjs
node docs/plans/2026-10-16-release-0070-checkpoint/verify-controls.cjs
```

The manifest covers 62 retained files, including release/API timelines,
pre-merge snapshots, ten phase/target evidence sets, corruption controls and the
real older-public control. Seven rehashed/missing-data mutations must be
rejected by the verifier.

This completes the selected Z80 correction checkpoint, including #629/#631,
#635 and #696. The pinned ecosystem execution work is tracked separately in
[#618](https://github.com/asm198x/asm198x/issues/618). Fresh native probes retained
in `followups.json` support [#714](https://github.com/asm198x/asm198x/issues/714)
(shift arithmetic) and [#715](https://github.com/asm198x/asm198x/issues/715)
(long packed-string warnings). No general compatibility percentage follows
from this release cohort.
