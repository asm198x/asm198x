# Verification

The 139 captured ca65/ld65 V2.18 executions cover 49 accepted and 90 refused
programs. The library matches their bytes and refusals; formatting preserves
bytes, section placement and symbols. CPU restoration, live/dead branches,
include state, repeat-entry register tokens, reflection, all eight indirect-Z
accumulator forms and newly reachable long branches are pinned. Human/JSON CLI
refusals retain included-file locations and all five previous output files.

Clean producer `c4173c37c248190db833b4a7e873bd73d5d5b16d` was replayed against
unchanged cc65 `e11fb5c39371046ebe25485f984f644c5a0d65d3`. The native 564-byte
output equals the pinned upstream reference, SHA-256
`8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec`.
The full target remains rejected: line 18 exposes the separate BBR/BBS
zero-page/relative operand family. All 16 minimal native controls and current
refusals are retained in `bit-branch-boundary.json`; their fresh producer build
log is adjacent. No whole-target equivalence is claimed. The focused result
leaves `ecosystem/results/current.json` unchanged.

`4510-after-selection/` retains the complete native artifact hex and fourteen
raw streams. Run `python3 verify_replay.py 4510-after-selection` in this folder
to check the source pins, artifact and streams. An actual byte mutation is
rejected by that verifier.

Final checks pass: 1,510 Rust tests with Lua (38 ignored), Clippy with warnings
denied, Rust formatting, generated docs, Ruff and 55 Node tests. The initial
minimal/fixture failures are retained. Package qualification requires all 139
native cases; removing the required case fails qualification.
