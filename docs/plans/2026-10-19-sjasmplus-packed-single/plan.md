# Admit native SjASMPlus packed single-quoted literals (#737/#856)

The Z80 tokenizer currently accepts only three-character single-quoted tokens,
and SjASMPlus data_items treats only double-quoted input as string data. Native
instead emits DB 'AB' as two bytes, while DB ('AB') is a packed numeric value.
These are separate admission paths using the existing numeric/string seams.

1. Retain fresh SjASMPlus 1.21.0 and clean preceding CLI identities and bounded
   native controls: lengths, doubled apostrophes, literal backslashes, UTF-8,
   data widths/aliases, scalar expressions/sign/IF, DEFINE/macros/includes,
   numeric separators, AF', malformed tokens and Pasmo isolation.
2. Add red-first tests/fixture at
   crates/asm198x/tests/sjasmplus_packed_single.rs and fixtures/sjasmplus-packed-single.json.
   Compare acceptance/bytes, formatter idempotence/symbols and actual native
   formatted output, plus five-output preservation in human/JSON modes.
3. Extend only existing quote/number seams in
   crates/asm198x/src/dialects/z80.rs and dialects/sjasmplus.rs. Gate tokenizer
   changes to SjASMPlus, decode single-quoted UTF-8 bytes/doubled apostrophes
   without double-quote escapes, and preserve native data-vs-scalar contexts.
   Keep Expr numeric, Pasmo behavior, numeric apostrophes and AF' isolated.
   No dependency, public enum/schema, common lexer or string value type.
4. Require the fixture cohort and omission control in
   scripts/test-candidate-package.cjs and scripts/candidate-package.test.cjs.
   Run focused/full Rust+Lua, Node, Clippy, fmt/Ruff/docs, native formatter
   differential, clean/preceding actual archives and corruption controls.
5. Replay the pinned affected SpecNext project from a clean source, retaining
   source/tool identities, all process streams and full bytes or an explicit
   remaining boundary. Do not claim this project uses the quoted form unless
   measured. Existing packed-overflow warning gap #715 and empty-value warning gap #857 stay separate.
   Byte-context validation must reject only wholly empty lists, preserving mixed
   empty items and wide zero values under #856.
6. Push checked work, inspect current exact-head checks and five actual packages
   with receipts and ten corruption controls, rebase-merge, then clean owned
   workspaces/builds/processes after verifying retained commits and evidence.
