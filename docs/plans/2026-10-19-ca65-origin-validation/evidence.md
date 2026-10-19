# Refuse ca65 origins beyond the native range (#817)

All seven native ca65/ld65 boundary cases match: four accepted and three
refused. The actual preceding counter-fix executable accepts $1000000 and
$FFFFFFFF incorrectly; the new code refuses both. Full native argv, streams,
statuses, config, sources and explicit executable hashes are retained.

Native DoOrg (pinned cc65 e11fb5c39371046ebe25485f984f644c5a0d65d3,
src/ca65/pseudo.c) checks 0..0xFFFFFF before EnterAbsoluteMode. The linked
path omitted that check. Add it after the existing evaluation and before
changing the pseudo-PC/mode, using the existing file/span error wrapper.
No dependency, public schema, source syntax or architecture changes.

The two new integration tests failed before the fix; the failure log is
retained. Final checks pass 1,563 Rust tests (38 ignored), 79 Node tests,
Clippy with Lua/all targets/warnings denied, fmt, Ruff and generated docs.
The initial fmt check found whitespace in the new test; rustfmt corrected it,
and the final fmt/focused checks pass. Accepted native boundary cases retain
formatter idempotence, bytes after reassembly and stable debug shapes.
Human/JSON included origin failures retain line 2 and the included path,
and preserve all five existing outputs and both source files.

The actual local Lua archive passes all 64 required package cohorts, including
19 commands in ca65_origin_bounds. The wrong-byte control fails exactly the
three expected byte-sensitive cohorts; the corrupt-archive control fails SHA
validation. The actual preceding archive fails only ca65_origin_bounds; its
other 63 cohorts pass. The mandatory-cohort omission control also passes.

This does not finish #815. The separate completed 57-case source-time matrix
has 30 native accepted and 27 refused cases. The preceding executable differs
on 21, and this range correction fixes exactly two with no new differences.
The other 19 remain open. A further 32 native expression controls constrain
the next design; the initial native timeout is research, never an expected
qualification result. See research.md for those limits.

Clean 4510 producer replay and all five actual host package reports remain
required before merge.
