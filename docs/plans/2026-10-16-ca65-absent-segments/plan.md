# Correct absent custom segment reservation before completing release

1. Reproduce native ld65 accepting a full PROGRAM area with an absent pinned TABLE. Check the previously accepted case against pre-#703 source and the current binary. Add ordinary and native cases for empty pinned/floating segments and zero-length segments touched by source.
2. Preserve the default NROM reservation while validating custom layouts against occupied segments. Extend existing layout resolution rather than adding a placement model. Verify native bytes and refusals, default NROM regressions, strict Clippy and workspace tests.
3. Push and merge through all exact-head checks/workflows. Record why the 0.0.68 native publication run was cancelled; do not move or rewrite its published crate/tag. Prepare and qualify the next patch release, then reconcile actual candidate/public archives and retain evidence.
4. Retain all commits/evidence and clean the task worktree and scratch/build directories.
