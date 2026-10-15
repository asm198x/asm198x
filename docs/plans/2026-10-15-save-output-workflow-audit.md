# Audit source-directed output placement and reporting (#641)

Compare current-source CLI behavior with the installed SjASMPlus reference for
bounded SAVEBIN output workflows. This is an audit, not an implementation change.
`decisions/multi-artifact-output.md` binds: library calls describe artifacts,
CLI calls write and report them, paths resolve at the output directory, and
absolute/parent paths are allowed. Do not turn intended path access into a bug.

1. Record exact CLI/reference executable hashes, versions, source commit and host.
   Reuse the optimized post-#671 binary, confirming bytes/version before probes.
2. In fresh directories, compare relative SAVE paths, output-directory changes,
   relative/absolute --outprefix, parent paths and absolute names. Compare exact
   saved bytes and resolved destinations against SjASMPlus; report limitations
   where the reference CLI has a different primary-output contract.
3. Exercise human and JSON reporting with valid output, missing parent and two
   explicit SAVE requests. Verify reported destinations agree with actual files,
   JSON assembly bytes retain machine-code meaning, and predictable errors
   preserve previous outputs. Repeat every observation with fresh directories.
4. Retain a rerunnable bounded Python probe, full JSON observations and report
   under docs/plans/2026-10-15-save-output-workflow-audit/. File specific issues
   only for reproduced defects, link existing issues rather than duplicate, and
   distinguish deliberate behavior, pass, failure and unavailable reference.
5. Verify probe determinism/positive controls, generated docs and lint where
   available. Push a report PR, merge only after acceptable completed CI, and
   clean the isolated worktree after verifying remote retention.
