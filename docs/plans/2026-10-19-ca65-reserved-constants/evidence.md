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
