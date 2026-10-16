# Preserve SjASMPlus quoted comment delimiters (#695)

The SjASMPlus line scanner chooses the earliest `;` or `//` without checking
quotes. Valid data is truncated before the shared data-list parser sees it.

1. Retain fresh native SjASMPlus probes in
   `docs/plans/2026-10-16-sjasmplus-quoted-comments/probe.py` and
   `crates/asm198x/tests/fixtures/sjasmplus-quoted-comments.json`. Measure quoted
   delimiters, real comments, mixed quote forms, register apostrophes, macros,
   repetition, and malformed source. Bound every subprocess.
2. Add red-first assembly, formatter, conversion and CLI regressions in
   `crates/asm198x/tests/sjasmplus_quoted_comments.rs`. Compare both CPU entry
   points with native bytes, preserve comments/source and previous images,
   and verify human/JSON paths and formatting idempotence.
3. Correct only `SjasmplusSyntax::strip_comment` in
   `crates/asm198x/src/dialects/sjasmplus.rs`, following measured quote and
   register boundaries. Leave data-string escapes and other grammar outside
   the correction. Keep real `;` and `//` comments working.
4. Append accepted native facts to normal Z80 verdict replay; regenerate docs.
   Run workspace tests, strict Clippy, rustfmt, Ruff, docs and coverage checks.
   Push a focused PR, require all exact-head checks/workflows to complete
   acceptably, squash-merge, update the tracker and clean task workspaces.
5. Review #641/#640 acceptance evidence and open correctness gaps. Link a
   completion assessment distinguishing the sampled audit, fixed defects,
   released-artifact evidence and the wider quality programme. Do not invent
   a percentage from issue counts or claim untested surfaces are complete.

No dependencies, public type changes, common lexer abstraction, or new grammar.

## Approved escape boundary

Native measurements showed that comment scanning alone could turn an error into
incorrect raw escape bytes. The agreed scope is the delimiter fix plus explicit
unsupported-escape errors, with full escape parity filed separately. The guard
belongs to SjASMPlus's existing data-item parser; common byte/word spellings now
use that same parser. No common lexer or public API is added.

The reference decodes 23 of the 95 printable ASCII backslash pairs inside double
quotes, including uppercase aliases and DEL (`\d`/`\D`); the other 72 pairs
remain literal. Single-quoted backslash is also literal. Exact native facts and
51 data-directive spelling probes are retained. The guard applies only inside
double quotes and refuses decoded pairs with `UnsupportedFeature`, preserving
old output. Unknown pairs and the single-quoted backslash retain native bytes.
The comment scanner understands escaped quote boundaries so formatting can
preserve source even where assembly explicitly refuses its payload.

## Verification

The new library, formatter/reassembly and CLI regressions failed with an
unexpected-double-quote expression error before the scanner fix. Re-enabling
the #693 conversion fixture also failed at target verification. An additional
red test demonstrated the raw-escape byte mismatch after a scanner-only change.
A single-quoted-backslash probe rejected an over-broad first guard; restricting
it to double quotes preserves the native-valid source.

All seven focused tests now pass, plus all five data-list regressions with the
old semicolon exclusion removed. The native formatter differential passes all
95 backslash-pair cases; that external-tool test is intentionally ignored by
default. Malformed source and unsupported escapes preserve previous images and
source in human/JSON modes across both CPU entry points. The accepted 84 byte
facts also enter normal Z80 replay.

The full workspace passed 1,434 tests, zero failures, 34 intentionally ignored.
Strict workspace/all-target Clippy, rustfmt, Ruff, generated docs and arbitration
coverage checks pass. The native-only formatter test passed separately against
SjASMPlus 1.21.0. The documentation generator refreshed ledger, compare and why
for the new facts; no coverage stamp or accepted shortfall was relaxed.
