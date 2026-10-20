# Audit and fix native quoted token-list admission (#872)

Native .left takes a quoted string as one token. The shared argument reader
currently removes its quotes and resolves backslashes before the ca65 token-list
reader refuses it. Capture actual grammar/bytes before choosing an admission fix.
The corrected #883 platform qualification continues on its frozen shipping head.

1. Retain native tool identities, exact inputs/processes/images in probe.py and
   native.json here. Cover .left/.right/.mid bounds, braced/unbraced quoted and
   scalar tokens, empty/embedded punctuation/backslash/quote boundaries, nested
   calls and each related predicate's actual grammar. Include selected/inactive,
   captured macro/repeat, include and source-preservation contexts.
2. Compare with frozen clean producer
   c478532a4f4fee2fd608a594f8e0f33a125b01a4 using compare.py. Classify all
   differences; keep wider token-origin, unsupported bare-token and string-escape
   gaps visible and separately tracked rather than silently broadening this fix.
3. Write the evidence-backed private design before source edits. Inspect
   crates/asm198x/src/dialects/text.rs (Arg/split_args/TextSyntax) and
   crates/asm198x/src/dialects/ca65_flat.rs (token_list/tokens/selectors).
   Preserve literal token spelling through the existing per-dialect grammar
   seam; avoid reconstructing a source literal from already decoded text.
   Reconsult string-and-text, source-order/capture and syntax decisions; stop
   before any new architecture/API/schema or significant convention change.
4. Add red-first native byte/formatter/diagnostic tests and fixture under
   crates/asm198x/tests/ca65_quoted_token_lists.rs and fixtures/, plus a required
   cohort and omission guard in scripts/test-candidate-package.cjs and
   scripts/candidate-package.test.cjs. Verify accepted literal bytes and refused
   shapes, native capture timing, output/source preservation and idempotence.
5. Freeze the focused CLI before full Cargo, then run full Rust+Lua, Node,
   Clippy/fmt/Ruff/docs, native formatting and prior/current actual archives with
   both corruption controls. Commit/push, verify a clean matching producer and
   complete 4510 replay, ship only this slice after #883, inspect exact-head CI,
   five actual platform receipts and ten controls before rebase merge/cleanup.
