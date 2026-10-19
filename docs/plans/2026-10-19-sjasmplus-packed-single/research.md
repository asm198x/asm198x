# Native packed single quotes and empty byte lists (#737/#856)

Native SjASMPlus uses a byte string for DB single quotes, but a signed packed
32-bit value in expressions and wide directives. The existing tokenizer admitted
only one character, and the data reader recognized only double-quoted strings.
The retained 293 controls expose 222 preceding byte/acceptance differences.

The SjASMPlus-only tokenizer now admits complete doubled-apostrophe literals.
The existing numeric seam packs UTF-8 bytes, with literal backslashes and signed
32-bit wrapping. Byte contexts retain multiple characters as strings and allow
single-character arithmetic. Wholly empty byte lists fail; mixed empty items and
wide zero values remain valid. Pasmo's grammar, numeric separators and AF' remain
isolated. No dependency or public AST/schema change is involved.

Both public SjASMPlus frontends match all 293 native acceptance/byte controls.
All 267 accepted cases have idempotent formatter round trips preserving symbols
and bytes; native SjASMPlus independently assembles each formatted source to the
same bytes. Included refusals preserve five outputs and both source files in
human/JSON modes on both CPU frontends. The mandatory actual-archive cohort runs
1,094 commands, with an omission guard and preceding-archive failure evidence.
Normal width warnings remain checked; native empty-value and packed-overflow
warning gaps remain explicitly tracked in #857 and #715.

Focused tests, all 90 Node tests, Lua/all-target Clippy, rustfmt, Ruff and generated
docs pass. Full Rust verdict replay and clean producer/project qualification
remain required before shipping.
