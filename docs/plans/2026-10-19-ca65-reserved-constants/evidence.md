# Refuse ca65 reserved immutable names (#775)

The retained matrix has 378 native ca65/ld65 cases across 6502 and 4510: 132
accepted and 246 refused. Register components and instruction-head spelling
follow the active CPU, with measured native aliases and BRU/LBRU exceptions.
Explicit root and qualified names keep their independently measured rules.
The formatter preserves inactive declarations without executing semantic guards.

An isolated workspace-package rebuild passed 1,532 Rust tests (38 ignored),
66 Node tests, Clippy/all features, fmt, Ruff and generated docs. A real
Lua-enabled archive passed all 51 required cohorts and rejected both controls.
The new cohort compares native acceptance/bytes and formatter/debug round trips;
included refusals preserve five output files and report their source location.
These staged results supersede the initial shared-target logs.

Rebase onto the completed #783 scope/width correction and repeat the combined
checks and clean unchanged-producer 4510 replay. Exact-head checks and five
actual packaged reports remain mandatory before merge. The preceding Intel
macOS network fetch failure and same-head successful retry are retained as
predecessor-fetch-red evidence; no qualification failure was bypassed.

No dependency, shared AST/API/schema/trait or source-reference shape changed.
Mutable definitions, attached labels and remaining phase differences are tracked
separately with native evidence.

After incorporating the complete #783 correction, the full isolated run passes
1,536 Rust tests (38 ignored), 67 Node tests and Clippy/fmt/Ruff/docs. The
combined Lua archive passes all 52 required cohorts and both negative controls.
These combined checks supersede the staged counts above.

Rebase onto merged #783 preserved the complete tested tree. Clean producer
6050cf2a9291bf7529d2a47252a3a5dcef2cee2b matches all 564 unchanged native
4510 bytes, SHA-256
8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec.
4510-after-reserved-constants retains complete comparisons and fourteen raw
streams, verified with the inherited content/identity verifier and changed-byte
negative control. The full ecosystem ledger is unchanged.
