# 0.0.67 acceptance and release checkpoint

This closes the current bounded #641 audit phase with explicit limits, and
delivers the post-0.0.66 source fixes through the existing release pipeline.
The broader #640 programme and Emu198x audits stay open.

1. Reconcile #641's five journeys and its three completion criteria against the
   dated baseline/follow-on reports and the completion review. Record a finite
   close-out in this plan and the final retained release report. Keep historical
   evidence unchanged; distinguish modern source for historical hardware from
   original period source and upstream build execution (#618).
2. Extend `scripts/test-candidate-package.cjs` using its existing required-case
   pattern to check the released disassembly, conversion, macro and quoted-data
   fixes and the approved unsupported-escape boundary. Keep missing/skipped/
   failed results unable to qualify. Validate the Node gate tests and demonstrate
   the new cases reject the actual older public archive.
3. Author only the new 0.0.67 entry in `crates/asm198x/CHANGELOG.md`; verify the
   release version, standalone web lockfile, generated docs/changelog and release
   workflow regeneration. Push the preparation to the existing #680 branch;
   inspect every exact-head check and triggered workflow before squash merge.
4. Observe the actual release workflow and inspect all five native candidate
   reports, source-lock receipts and deliberate corruption controls. Confirm
   hosting follows successful qualification. Resolve the actual release tag.
5. Dispatch `test-linux-packages.yml` for published-version 0.0.67, using the
   same case contract and no rebuild of public artifacts. Inspect all five
   public reports and controls; require exact candidate/public archive and
   executable hash identity and the published producer source lock.
6. Retain reports, plans, acquisitions, timeline and a fail-closed evidence
   verifier in `docs/plans/2026-10-16-release-0067-checkpoint/`, through a separate
   evidence PR after release. Include missing/changed-report negative controls.
   Link the audit close-out and outstanding children from #641/#640; close #641
   as the bounded audit once all its stated criteria have evidence. Do not mark
   the wider programme complete. Inspect and remove task worktrees, scratch,
   build caches and processes after verified remote retention.

No new dependency, release architecture, or support promotion is required.
