# 0.0.71 native package checkpoint

All five native candidate/public archive pairs pass the frozen 33-case cohort: 330 passing named cases across macOS ARM/Intel, Linux ARM/x86-64 and Windows x86-64. Actual release qualifications completed before hosting; the separate tag-pinned public-download run verifies the same archive, executable and producer-receipt identities. Wrong-byte and corrupt-archive controls fail in all ten target/phase combinations.

- Release: [asm198x-v0.0.71](https://github.com/asm198x/asm198x/releases/tag/asm198x-v0.0.71), published 2026-10-16 18:38:21 UTC.
- Source/tag commit: `60babae495e4e37963a5bbec23068ae21ff71eb3`.
- Committed Cargo.lock SHA-256: `0ee4b612d610b2d581a5b32e1666f431bdfcf55b99cfaed65841aa48e369958a`.
- Actual release qualification: [38947879894](https://github.com/asm198x/asm198x/actions/runs/38947879894).
- Actual public downloads at the immutable tag: [38948976392](https://github.com/asm198x/asm198x/actions/runs/38948976392).
- Release PR [#724](https://github.com/asm198x/asm198x/pull/724), head `844f5380c60b40a67ae49b67d413d6fa33a120ad`: all 38 checks and seven triggered workflows completed successfully or were explicitly skipped/neutral before merging.
- Assembler fixes [#723](https://github.com/asm198x/asm198x/pull/723), [#726](https://github.com/asm198x/asm198x/pull/726), [#727](https://github.com/asm198x/asm198x/pull/727), and audit-retention correction [#729](https://github.com/asm198x/asm198x/pull/729): each exact final head had 33 completed checks and five completed workflows before merging.

The three new package cases replay native ca65 used extents, SjASMPlus logical expressions and signed 32-bit shifts, including preservation on refusal. The actual older public 0.0.70 archive passes the other 30 current cases and fails exactly these three corrections. Its historical 30-case qualification remains unchanged.

The separate complete native audit at `../2026-10-16-next-project-boundaries/github-checkpoint/` retains the actual 84-file upload, 80 process logs, all ten pinned target outcomes, 646 live Linux shift controls and nine rejection controls. It records six final-artifact matches, two Asm rejections now tracked by #722/#725 and two failed ca65 controls tracked by #718. The earlier partial upload is retained separately and was not promoted. This release completes #716, #717 and #714; it does not establish general compatibility or close those next project boundaries.

The source-lock receipts are unsigned producer evidence. They bind the observed archive/executable to the committed lock and checkout; the complete compiled dependency graph remains explicitly unknown.

```sh
node docs/plans/2026-10-16-release-0071-checkpoint/verify.cjs
node docs/plans/2026-10-16-release-0071-checkpoint/verify-controls.cjs
```

The manifest covers 62 retained files, including release/API timelines, pre-merge snapshots, ten phase/target evidence sets, corruption controls and the actual older-public control. Seven missing/rehashed mutations must be rejected without importing the evolving package gate.
