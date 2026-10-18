# Scoped immutable constants and source preservation

The reader put a constant's qualified lookup key in its authored-name field.
The formatter therefore changed `VALUE=7` inside a scope to `inner::VALUE=7`,
which the reader then refused. Authored and semantic names now remain separate
on the existing Symbol seam. Qualified definitions resolve through the existing
scope resolver and validate the complete target parent; global qualification
is retained. No shared AST or public API changed.

Immutable declarations are tracked during the selected projection sweep, even
when their RHS cannot fold. Repeated declarations, including identical values,
now refuse before output writes. Untaken branches remain unexamined. Existing
label collision checks still apply. Loop-bound constant RHS values are folded
at their live iteration before ordinary statement substitution.

Formatter parsing keeps `.proc`'s authored defining directive without emitting
its implicit address label twice. Assembly projection retains that label. It
also preserves external qualified names, because formatting does not open
includes and therefore cannot validate scopes defined there.

Unnamed `.scope` instances use the existing unique inaccessible scope stack.
The scope-reference path now uses a delimiter distinct from the separator in
synthetic scope identities and cheap-local keys. The full-suite cheap-local
regression exposed the ambiguity and now passes alongside unnamed forward-label
and nested-repeat controls.

The native ca65/ld65 V2.18 capture has 49 qualifying cases: 32 accepted and 17
refused. Rust tests compare native bytes, formatter idempotence, reassembled
bytes, section bases and symbol addresses. Human/JSON include errors retain
file/line locations and preserve the previous image and every sidecar. A
missing include does not prevent formatting qualified declarations/references.
Every CLI call has a five-second watchdog that kills and reaps its child.
The retained red log proves the original formatter failure is detected.

Actual-package qualification requires this cohort and rejects omission. Each
host executes 145 CLI commands: one assembly for every case and three extra
formatter/round-trip commands for every accepted case, checking native bytes
and section/symbol identity. The required case total is 44.

Local verification: 1,519 Rust tests passed, 38 ignored; 59 Node tests passed.
Clippy with warnings denied, Rust/Python formatting, Ruff, and generated docs
checks passed. Compressed logs retain complete verification output.

Separate native findings #759 and #760 are retained in `deferred-native.json`
and are not qualifying cases in this slice. They concern unnamed `.proc`
closers and token substitution on repeat definition lines; the next slice
addresses them. Further measured `.set` and `:=` gaps are tracked as #761/#762.

Clean producer `cd8284a992523e8020047f32db0a0a3a70677e45` still matches all
564 bytes of the unchanged pinned cc65 4510 target, SHA-256
`8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec`.
`4510-after-scoped-constants/` retains complete native/Asm bytes and fourteen
raw streams. `verify_replay.py` checks content, source and producer identities,
process completion, metadata agreement and changed-byte rejection. The complete
ecosystem cohort report remains untouched.
