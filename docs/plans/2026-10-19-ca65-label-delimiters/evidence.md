# Preserve ca65 attached label delimiters (#781)

The initial token may end its label at a single colon with no following space.
A private prefix splitter skips :: pairs and is shared by label binding and
structural block-head recognition. Only confirmed macro-call input is spaced;
ordinary authored source and its diagnostic columns remain untouched.

There are 24 retained native ca65/ld65 cases: 22 accepted, two refused. The CLI
tests check bytes/status, formatter idempotence/reassembly, debug shape and
included failures preserving all five outputs. A source-derived column control
covers attached, spaced and indented prefixes. Leading labels on macro
definitions are retained separately as #782, including spaced definitions.

The isolated full workspace run passed 1,535 tests (38 ignored); 67 Node tests,
Clippy/all features, fmt, Ruff and generated docs passed. The actual Lua-enabled
archive passed all 52 required cohorts and rejected wrong-byte and corrupt
archive controls. The new cohort runs 90 CLI commands. These are staged local
results on the pre-scope-correction parent. Rebase onto the completed parent,
repeat combined checks and unchanged clean-producer 4510 replay, then require
exact-head CI and all five real package reports before merge.

The retained cache collision and column red controls document why isolated
build targets and source-derived diagnostic positions are required. No public
AST/API/schema/trait or dependency changed.
