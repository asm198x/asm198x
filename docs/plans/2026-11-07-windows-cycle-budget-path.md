# Windows cycle-budget diagnostic assertion

The Windows job on release PR #1065 correctly rejects the included routine's
six cycles against a five-cycle limit. Its diagnostic names
`sub\cycles.inc:6:`, but the test requires `sub/cycles.inc:6:`.

1. Normalize path separators only in the diagnostic assertion in
   `crates/asm198x/tests/ca65_absolute_debug.rs`. Keep the filename, line,
   rejection status, budget message and output-preservation checks intact.
2. Check the recorded Windows diagnostic fails the old assertion and passes
   the normalized assertion; verify wrong filenames and line numbers fail.
3. Run `cargo test -p asm198x --test ca65_absolute_debug`, formatting and the
   affected test's Clippy check. Open a checked PR; Windows verification remains
   required before declaring the release unblocked.

No production behaviour changes. Preserve the older task worktrees.
