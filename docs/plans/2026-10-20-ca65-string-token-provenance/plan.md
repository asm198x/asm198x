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
