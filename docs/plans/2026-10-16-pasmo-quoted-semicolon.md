# Pasmo quoted semicolons (#690)

Keep semicolons inside Pasmo data strings and strip real trailing comments.
The cause is `PasmoSyntax::strip_comment` cutting at the first semicolon before
the Z80 data parser sees the closing quote. Both Pasmo frontends use it.

1. Measure quote/comment boundaries with native PasmoNext; retain a bounded
   probe in `docs/plans/2026-10-16-pasmo-quoted-semicolon/probe.py` and its facts
   in `crates/asm198x/tests/fixtures/pasmo-quoted-semicolon.json`.
2. Add native-byte, formatter round-trip, CLI human/JSON, and failed-output
   preservation regressions in `crates/asm198x/tests/pasmo_quoted_semicolon.rs`.
   Run them before the fix to demonstrate failure.
3. Correct only Pasmo's comment boundary in
   `crates/asm198x/src/dialects/pasmo.rs`; retain register apostrophes and the
   existing comment grammar. Replay the measured cases in both frontends.
4. Run the workspace tests, strict Clippy, rustfmt, and generated-doc checks.
   Push a focused PR, verify its exact-head CI, squash-merge, and clean up.

No dependencies, public type changes, macro grammar changes, or new escapes.

## Verification and boundaries

The red run failed three behavioral regressions with an unexpected-double-quote expression error; the existing failed-output
check passed. After the comment-boundary fix all four regressions pass across
seven native-valid fixtures and one native refusal. Both Pasmo frontends match
native bytes, formatting is idempotent and reassembles identically, CLI human
and JSON paths preserve source, and invalid input preserves the previous image.
Seven native facts were added to the normal Z80 verdict replay corpus.

An exploratory mixed list containing the double-quote character literal before
another value fails separately in data-list parsing (an unexpected-comma expression error). That is outside comment stripping: use an ordinary character
in this fix's mixed-list fixture and retain the delimiter failure for follow-up.
Native probes also accept multi-character single-quoted strings and backslash
escapes; this change does not add those data-literal features.

Strict workspace/all-target Clippy, rustfmt, Ruff, generated documentation and
arbitration coverage checks pass. The generator refreshed ledger, compare and
why pages after the new verdict facts; no coverage stamp changed.

The full workspace run passed 1,419 tests, zero failures, 33 ignored.
