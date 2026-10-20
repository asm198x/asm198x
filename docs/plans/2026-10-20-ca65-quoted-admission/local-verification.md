# Quoted-token admission verification

The full final workspace/Lua suite passes 1,627 tests (38 ignored). All 107 Node
checks, all-target Clippy with warnings denied, rustfmt, Ruff and generated docs
checks pass. Earlier expanded coverage correctly exposed #900 with
`ascii_44_direct: left: false, right: true`; its full failed log is retained.
The final required case retains #900 as an explicit safe refusal, checking its
nonzero exit and preserved outputs, rather than dropping or ignoring the case.

The 549-case native audit has 548 native matches plus that known safe refusal.
It removes 143 wrong acceptances and two first-error location mismatches. The
new required cohort runs 1,647 commands, with native bytes, first-error lines,
four previous outputs on refusal, canonical/idempotent formatting and debug
symbol/section round trips. Included captures also check both quoted token
forms across all three CPU readers, with the included path and original line.

Thirty earlier #835 literal-boundary exclusions are promoted to required cases:
798 cases, with the same 477 native acceptances. The 264 remaining exclusions
are all the separate flat conditional/repeat gap #895. Nine raw control-byte
string cases retain their native bytes, independently of character admission.

A clean producer, final native replays, local archives/corruption controls, fresh
4510 replay, exact-head CI and all five native archive reports remain required
before merge. The approved lexical decision is recorded separately from #900's
operand-boundary follow-up.
