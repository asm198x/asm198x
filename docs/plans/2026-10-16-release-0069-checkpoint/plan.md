# Corrected native release checkpoint

1. Wait for #705's exact-head checks and every triggered workflow, then squash-merge. Wait for release-plz to prepare the next patch after immutable 0.0.68; author the new `crates/asm198x/CHANGELOG.md` entry and verify version/lockfile/ledger/changelog consistency.
2. Merge the release PR only after all exact-head checks/workflows complete. Inspect actual tag-triggered archive qualification and hosting on every native target. Dispatch `test-linux-packages.yml` with the published version and inspect the downloaded public archives, not a rebuild.
3. Retain both phases' reports/plans/acquisitions/corruption controls, source-lock receipts, public asset digests, pre-merge checks and workflow timelines in this folder. Freeze the 28-case verifier and demonstrate missing/changed evidence is rejected. Keep 0.0.67 evidence unchanged and retain 0.0.68's interrupted checkpoint as such.
4. Publish this evidence through a PR, update the issue outcomes, verify task commits are retained on remote refs/main, then inspect and remove the task worktree/build/scratch directories and confirm task processes have exited.
