# HuC6280 macro formatter verification

All 32 native macro programs assemble and round-trip byte-identically and
idempotently, retaining definitions, calls and unread template markers. The
65816 controls retain their existing behavior. Two included-macro range errors
retain the native file/line and four existing outputs before and after formatting.
Their pre-existing missing definition frames remain separately tracked in #904.

The HuC6280 walker follows the existing 65816 convention, with explicit formatter
mode guards: authored macro bodies become ordinary source-bearing AST nodes,
and case-sensitive known calls stay calls. Assembly expansion is unchanged.
No whole-file preservation fallback or new macro grammar is introduced.

All 203 formatter promotions have native-byte and idempotent round trips: 185
actual #821 refusals fixed here, plus 18 already resolved #833/#834 exclusions
removed separately under #902. The affected Rust and package loops now require
formatting instead of accepting formatter-gap flags. The new archive cohort
retains 32 programs plus two included errors, with 130 CLI commands.

The complete Rust/Lua workspace passes 1,631 tests (38 ignored); all 109 Node
checks, all-target Clippy, rustfmt, Ruff and generated docs pass. The first build
reported a missing BTreeSet import; that failed log is retained. A checked build
completed before the final native replay; the stale-binary trial is not used.

Clean producer 164dbc71e92b517afec5141a09e57e1da76ce845 has CLI SHA-256
033b1269f942f071e93a6f1defd0ca50c6222ad3786e19c7afcf61cd1a633bb3.
Its replay matches all 32 programs, 864 operands, 549 quoted-admission cases,
396 text values and 124 assertion boundaries. Earlier native matches are all
retained, without unexplained new admission.

All 94 local archive cohorts pass, including 130 new macro/error commands.
The predecessor fails exactly the four affected formatter cohorts. Wrong bytes
fail the three expected byte/determinism/structured-success controls; a corrupt
archive fails SHA-256. Fresh 4510 replay matches all 564 bytes and fourteen
streams, rejects changed bytes and leaves no disposable work. Complete clean
identities/replays, archive controls and streams are retained here. All exact-head
CI and five actual native archives remain required before merge. #904 is next.
