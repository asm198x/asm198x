# Byte-string provenance implementation plan

The user approved the private ca65 byte-token design on 2026-10-20.
Keep #886 byte preservation before #887 literal validation and #872 quoted
admission. Retain the 71/287/623-case native audits as independent evidence.

1. Preserve raw quoted argument spelling in `dialects/text.rs` alongside the
   existing cooked text. Update exhaustive patterns in `ca65_flat.rs` and
   `rgbasm/mod.rs` without changing their successful results. Test one/two
   backslashes, escaped quotes, UTF-8, nesting and existing cooked accessors;
   run the full Rust suite, Clippy and fmt before retaining this foundation.
2. Add `ca65/string_tokens.rs`; connect its assembly lifetime to `ca65_flat.rs`,
   `ca65/project.rs`, `ca65/mod.rs`, `ca65_816.rs` and `ca65_huc6280.rs`.
   Inventory exact string consumers first. Verify isolation, injected/unknown
   token refusal, selected macro/repeat/include lifetime and source preservation.
3. Convert ca65 construction/formatting and data consumption to byte values.
   Re-run all 71 and 287 retained native cases against a frozen CLI; account for
   every difference, preserving #795 as separate. Check byte precision, padding,
   all 1..255 characters, nested functions, .asciiz and numeric refusals.
4. Validate authored literal boundaries for #887, then admit quoted token lists
   for #872. Compare the complete 623-case audit, including adjacent literals
   and authored one/two-backslash controls.
5. Add required candidate cohorts and omission guards. Verify full Rust/Lua,
   Node, Clippy/fmt/Ruff/docs, formatter/debug/diagnostic invariants and controls
   that fail with the known lossy predecessor. Measure repeat/macro cost.
6. Export each qualified source slice onto actual main in a clean shipping
   worktree. Commit/push, inspect exact-head CI and all five actual packages
   with ten controls, run clean 4510 replay, rebase merge and clean up.

Update the binding string-layer decision with the approved byte-token boundary
when its implementation lands. No Expr, public API/schema, dependency, inactive
lexical scope, source-order numeric lookup or general identifier-capture change.

## Package qualification slice

The source implementation and fixtures are one byte-preservation slice for
#886/#889/#890. The 9 existing unbraced macro-argument refusals are retained in
full native/predecessor audits and explicitly excluded under #891. #887's three
raw literal refusals and #795's scoped query remain separate; quoted token-list
admission still has exactly 255 known differences under #872.

Verified additional files: `tests/ca65_string_bytes.rs` and native fixtures
`tests/fixtures/ca65-{string-bytes,string-consumers,flat-string-bytes}.json` below
`crates/asm198x/`; `scripts/test-candidate-package.cjs` and
`scripts/candidate-package.test.cjs`. Require three new package cohorts with
287/102/574 cases, 274/90/548 native acceptances and 1,109/372/2,218 CLI commands.
The full package cohort count becomes 89. Assert full native audit accounting,
sidecar preservation on refusal, canonical/idempotent formatting, native images,
stable section/symbol shapes and absence of private handles in all text outputs.

Six native marker-admission controls are included in the consumer cohort:
quoted/unquoted marker-looking source under 6502, 65816 and HuC6280. A preceding
constructor allocates the same apparent ID, proving literal bytes stay literal
and authored bare handles cannot resolve it. Text-output marker assertions apply
to cases without an authored marker; those six instead compare the exact native
image/refusal and complete output/formatting invariants. Metadata must agree
with the actual authored source. The qualified total is 963 cases, 912 accepted
and 51 refused, with 3,699 CLI commands across the three cohorts.

The remaining computed quote payload/path cases are tracked in #892.

## Named-index safety boundary

Four additional native controls reproduce two newly admitted wrong images in
the first producer: a shadowed scoped index and a missing reference event.
Keep named indices refused under #795; allow literal arithmetic for #890.
`ca65_flat.rs` checks the parsed index for symbol references before evaluating
it. `index_scope_probe.py` and its native/predecessor/prototype records retain
all four controls. The consumer fixture now has 106 cases, including both
explicit #795 refusal controls and two native-matching arithmetic cases.
The three cohorts cover 967 cases: 965 match native, and two preserve declared
legacy refusals. Accepted counts are 274/92/548; command counts 1,109/382/2,218.
Re-run full qualification and require the pre-guard producer archive to fail
exactly the consumer cohort. The first producer is negative evidence for this
boundary and cannot qualify the final shipping source.

The 26-case numeric-function arity audit preserves native empty-call refusals,
argument types, inactive captures and empty calls retained as token lists.
Recognition probes use line zero; real empty numeric calls enforce arity.
The consumer cohort is now 132 cases, with 104 accepted, 444 CLI commands,
and the same two explicit #795 controls. Combined: 993 cases, 926 accepted,
65 native refusals plus two retained #795 refusals, and 3,771 CLI commands.
Re-run all checks after this final compatibility guard.
