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

The clean final producer is 3606d654387e72b3287cc039e7d3b55eca5fd756, CLI
SHA-256 7468d49a7e9adf0aa69b3a1611765ab3ecf618c71f36a7b1ae83cccd2d93457b.
Its preceding clean build 2f094aa5 has identical Rust/xtask sources and executable
bytes. The only intervening change corrects the existing identifier cohort count
from an accidental 549 back to 165. The initial archive failed closed on
`165 !== 549`; its report remains. All 92 corrected local cohorts pass; both
corruption controls reject as intended. The preceding archive passes all 90
unaffected cohorts and fails exactly quoted admission and literal boundaries.

The frozen producer retains every match across the earlier native byte, text,
numeric, scope and lifetime audits, with no unexplained new acceptance. All
396 text-value cases, 124 assertion-boundary cases and nine additional raw
control-byte string cases match. The 623 original quoted-token native facts
are also retained as compressed primary evidence.

Fresh 4510 replay matches all 564 bytes and fourteen raw process streams and
rejects a changed-byte control. That replay builds clean 2f094aa5; the retained
source-equivalence record proves the later checker-only change leaves Rust,
xtask and CLI bytes identical. The audit runner removed its disposable build work; its absence is verified. All exact-head CI and five actual native archives remain required
before merge; those reports will be retained after inspection.
