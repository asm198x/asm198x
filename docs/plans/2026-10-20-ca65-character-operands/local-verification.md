# Native character-operand verification

All 864 native assembly outcomes match: 672 acceptances and 192 refusals,
removing 234 existing safe refusals across all three CPU readers and all four
data directives. Every negative case keeps the native first-error line. CLI
tests also check four existing outputs on refusal and macro frames after
expansion, JSON/file byte agreement and official Debug198x parsing.

The data readers reuse the existing native-aware assertion scanner, moved to
`ca65_flat::split_fields`; assertions retain their two-separator header limit.
Shared MOS parsing and native lexical admission are unchanged. The selected
comma-character case in the 549-case quoted cohort now matches native, and
its #900 exception is removed from both Rust and package checks.

Canonical/idempotent byte/debug round trips cover 560 accepted cases. The 112
HuC6280 macro cases remain checked for assembly, JSON/native bytes and official
debug parsing; both the clean predecessor and trial reproduce their formatter
refusal under #821. Complete paired traces are in `formatter-gap-821.json`.
The required new package cohort runs 2,544 commands.

The complete Rust/Lua workspace passes 1,628 tests (38 ignored); all 108 Node
checks, all-target Clippy with warnings denied, rustfmt, Ruff and generated docs
pass. The earlier focused formatter failure is retained verbatim; it exposes
#821 rather than a new assembly failure.

Clean producer 6385342e512bf9696b9541e9e24e4d973a575daa has CLI SHA-256
721454a8f7825843413ff88525be1e6a02fa12898d65a0b33b304926156ce7cd.
Its complete replays match all 864 operand cases and all 549 quoted-admission
cases. The earlier native audits lose no match and introduce no unexplained
new acceptance; all 396 text values and 124 assertion boundaries still match.

All 93 local archive cohorts pass. Wrong-byte controls fail exactly the three
expected byte/determinism/structured-success checks, and corrupt archives fail
SHA-256. The preceding archive passes every unaffected cohort and fails exactly
quoted admission and character operands. Fresh 4510 replay matches all 564 bytes
and fourteen raw streams, rejects a changed-byte control and leaves no build
work. Complete clean identities/replays, package controls and streams remain
inspectable here. All exact-head CI and five native archive reports remain
required before merge. The next slices address #821 and #902.
