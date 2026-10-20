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

Clean producer/native replays, local archive controls, fresh 4510 verification,
exact-head CI and all five actual native archive reports remain required before
merge. The following slices address #821 and stale closed formatter exemptions.
