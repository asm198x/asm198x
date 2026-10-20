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

Clean producer/native replays, local archives and corruption controls, fresh
4510 replay, exact-head CI and all five native package reports remain required
before merge. #904 is the next diagnostic-context slice.
