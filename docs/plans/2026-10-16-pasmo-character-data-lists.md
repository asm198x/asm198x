# Preserve character literals in data lists (#693)

Pasmo data lists containing a quote or comma character literal fail even
though native PasmoNext accepts them. Trace the data-list splitter before
changing it: it currently toggles string state on every double quote and
splits every comma outside that state, including those inside character literals.

1. Retain fresh native PasmoNext and SjASMPlus probes under
   `docs/plans/2026-10-16-pasmo-character-data-lists/` and fixture facts in
   `crates/asm198x/tests/fixtures/pasmo-character-data-lists.json`. Cover quote,
   comma and semicolon characters, neighbouring numeric/string values, real
   comments, word data, repetition, and malformed lists.
2. Add library, formatter and CLI regressions in
   `crates/asm198x/tests/pasmo_character_data_lists.rs`. Compare native bytes,
   both Pasmo frontends, idempotent formatting/reassembly and human/JSON output;
   preserve previous images and source on refusal. Demonstrate red first.
3. Correct `split_data_items` in `crates/asm198x/src/dialects/z80.rs` to keep
   supported character literals intact. Check its SjASMPlus consumer against
   native facts. Do not add string escapes or multi-character quoted grammar.
4. Add accepted facts to normal verdict replay and regenerate affected docs.
   Run workspace tests, strict Clippy, rustfmt, Ruff, docs and coverage checks.
   Push a focused PR; require all exact-head checks and triggered workflows to
   finish acceptably before squash merge. Update #641 and clean task files.

No dependency, public API, AST, or dialect grammar change is needed.

## Verification

Three initial behavioral regressions failed with an unexpected-comma expression
error in library assembly, formatted-source reassembly and the CLI. The existing
malformed-source output-protection regression passed. After the splitter fix,
all five focused tests pass, including verified conversion byte parity.

Fresh PasmoNext v0.1.3 and SjASMPlus 1.21.0 observations retain exact source,
commands, executable digests, raw streams and outputs: 17 accepted cases and
four refusals. The native refusals sometimes write partial files; Asm198x's
CLI still preserves its previous image. Both Pasmo frontends and SjASMPlus
match accepted bytes and retain idempotent, byte-equivalent formatting.
Accepted byte facts also enter the normal Z80 replay corpus.

Strict workspace/all-target Clippy, rustfmt, Ruff, generated documentation and
arbitration coverage checks pass. The generator refreshed ledger, compare and
why pages after the new verdicts; no coverage stamp was relaxed.

The SjASMPlus quoted-semicolon fixture remains outside this splitter change:
its independent line-comment scanner cuts inside quoted text before data-list
parsing. Fresh follow-up probes also reproduce this for a quoted `//`; these
need a separately measured dialect comment-boundary correction. Conversion
checks here exclude that independently failing semicolon source.

The full workspace run passed 1,427 tests, zero failures, 33 ignored.
