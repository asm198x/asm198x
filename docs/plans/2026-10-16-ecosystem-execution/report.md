# Ecosystem execution checkpoint (#618)

The ten admitted targets now have reviewed complete build plans and retained native results. Six complete final artifacts match, two targets stop at an Asm198x boundary, and two native controls fail before an Asm result can be interpreted. This measures this pinned corpus, not all source compatibility.

The native capture is `ecosystem/results/current.json`; its producer revision is retained on `evidence/ecosystem-producer-2026-10-16`. The development producer reported 0.0.69; source and executable hashes identify the actual build. Public 0.0.70 package qualification is retained separately in the release checkpoint.

`native-blocked-replay.json` records the clean 0.0.70 source replay. `native-blocked-control.json` hashes both reports and records the trap control: all eight native executable names were replaced with task-owned wrappers which append their name and exit 99. Invoking each wrapper first proved it could fail. Replay added no calls, re-evaluated all eight native-qualified targets, preserved all six successes and reproduced the two known boundaries. Its temporary build/source copies were removed automatically. Native capture and replay timestamps remain separate.

The known boundaries are ca65 unfilled partial output extents (#716), SpecNext ASSERT expressions (#717), and ca65 tool-generation controls for 6502DTV/65CE02 (#718). The 4510 native adapter also matches the pinned upstream `.ref` exactly (564 bytes). An initial HelloAmi framing mismatch was corrected in the plan before the accepted capture; both final hunk executables match (244 bytes).

Verification: 1,478 workspace tests passed (37 external-tool tests ignored); Clippy and format checks passed. Fifteen runner tests cover wrong/missing artifact evidence, stale/duplicate/incomplete records, source mutation, path/symlink escapes, executable proxies, timeouts including helper processes, bounded logs, final ROM selection and incomplete publication cohorts. The generated ledger and 65-page book pass regeneration checks. Ordinary CI repeats source acquisition and Asm replay; a dedicated weekly/manual workflow captures actual native results and retains evidence even when controls fail.

The actual GitHub workflow checkpoint is added after the implementation lands. Failed native controls remain visible and fail the capture command; they are never promoted to Asm compatibility successes.
