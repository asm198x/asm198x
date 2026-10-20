# Preserve ca65 byte strings as source tokens

Status: approved by the user on 2026-10-20; private ca65 byte-token design.
Scope: #886 and #887, then the quoted-string portion of #872.

## Evidence and old failure modes

The [71-case audit](https://github.com/asm198x/asm198x/tree/7d0d3ba7/docs/plans/2026-10-20-ca65-format-argument-types)
finds four wrong computed-string images, three malformed raw data acceptances,
and one existing #795 scoped lookup refusal. Numeric .sprintf argument typing
already matches native and needs no change.

The [287-case byte sweep](https://github.com/asm198x/asm198x/tree/99384250/docs/plans/2026-10-20-ca65-format-argument-types)
finds 145 differences. Examples:

- .sprintf("%c",34): native 22, current 5c22.
- .sprintf("%c",92): native 5c, current 5c5c.
- .sprintf("%c",255): native ff, current c3bf.
- .sprintf("%.1s","é"): native c3, current c3a9.
- Raw "a""b" and "a"+"b" data operands are refused natively but emitted here.

The implementation loses information at two boundaries. Arg::Text retains only
cooked UTF-8 text, so raw literal spelling is unavailable. Folded::Text is then
serialized as C-escaped quoted source. ca65's data reader instead preserves
literal backslashes; the MOS string_literal helper only checks outer quotes.
A quote/backslash encode toggle cannot preserve every case. Vec<u8> is required
for native byte values and precision cuts that are not valid UTF-8.

## Alternatives

1. **Private ca65 byte-string tokens (recommended).** Retain raw spelling on
   shared text arguments, keep other dialects' cooked behavior, and carry ca65
   computed byte strings through an assembly-owned private token store. Existing
   shared text folding and macro substitution continue to operate once. ca65
   consumers resolve the typed values rather than reparsing escaped literals.
2. **Shared typed text stream.** Change the shared text/expansion result into a
   typed stream and migrate every dialect/consumer together. This could serve
   future consumers, but broadens the migration and its reference obligations
   beyond the demonstrated ca65 need.

## Recommended boundary

`text.rs` retains both the authored spelling and existing cooked value for a
quoted argument. Existing accessors keep their cooked behavior for other
frontends. ca65 explicitly reads native literal bytes from raw spelling.

A new private `ca65/string_tokens.rs` owns immutable byte-string values and
source provenance for one assembly. Computed .concat/.string/.sprintf results
use opaque internal handles through the existing Folded::Bare/text and macro
mechanics. This follows the existing private stand-in convention, but adds a
new assembly-owned value store and therefore needs design agreement.

Handles are compiler-owned, cannot be authored as ca65 syntax, and are resolved
only in trusted expanded input. Unknown handles fail closed. Quoted source that
contains marker-looking bytes stays literal data. Values are never recovered by
lossy UTF-8 conversion or by globally unescaping authored source.

Linked ca65 owns the store in its existing reader/sweep lifetime. Flat 65816 and
HuC6280 expansion returns a private ca65 wrapper that keeps the store alive
through parsing. Do not add ca65 payload fields to shared macros::Expansion or
copy the shared expansion/folding mechanics.

ca65 text functions interpret both authored and computed strings as byte-string
tokens. Numeric positions continue to reject strings, including computed tokens.
%s precision/width use native byte counts; %c retains the measured 1..255 range.
Native token-list predicates compare token kind and byte values. Selectors
preserve selected token identity instead of turning it into a quoted source
approximation. The quoted-string admission change lands only after this path
has independent byte qualification.

Data consumers resolve string tokens directly to the existing numeric byte
operations. The ca65 literal reader also verifies that one authored literal
consumes its complete data operand, rejecting trailing quote/operator tokens.
A computed quote is a valid string value; an authored escaped quote follows
native lexical rules. These cannot share a naive outer-quote check.

Inventory every ca65 string consumer before changing it: data/asciiz,
include/incbin paths, segment/CPU names, messages, identifier/text functions and
token-list functions. Keep their current contracts and native refusal controls.
The verified .word/.dword/immediate string refusals remain refusals. Textual
contracts remain UTF-8; arbitrary binary string values are materialized only
through consumers whose native grammar accepts them.

Keep authored source/provenance alongside internal tokens. No handle may appear
in user files, diagnostics, symbol names, listings, JSON or Debug198x records.
The formatter continues using the syntax-only reader and canonical AST renderer.
No whole-file preservation fallback or formatter option is introduced.

## Binding decisions and scope limits

Expr remains numeric i64. String bytes remain source-token data handled before
layout; no string-valued expression or layout callback is added. Shared macro
and text mechanics remain shared, with per-dialect grammar/adapters.

Clarify `decisions/string-and-text-layer.md` after agreement: native byte-string
tokens may retain values through the source pre-pass instead of being forced
through UTF-8/C-escaped source. This preserves the numeric-expression decision.

Selected/captured reading and existing numeric-only inactive lexical validation
remain binding. This slice does not expand skipped-source lexical errors (#835),
change source-order numeric queries (#795), or implement general register/prefix
identifier provenance (#848/#853/#876). Record those boundaries explicitly.

## Delivery and verification

1. Add raw literal spelling to private text arguments; mechanically preserve all
   other dialect accessors/semantics. Verify full Rust and existing references.
2. Add the private byte-token store and thread it through linked and flat ca65
   expansion/readers. Test handle lifetime, isolation, unknown/injected handles
   and preservation across selected macros/repeats/includes.
3. Make ca65 string construction/formatting byte-based and consume string tokens
   at existing data/string consumers. Qualify all 71 and 287 retained controls,
   all 1..255 bytes, range refusals, UTF-8 precision/padding, nested calls and
   .asciiz. Test every old failure mode against the new path.
4. Tighten authored ca65 data-token boundaries for #887, then admit quoted string
   tokens for #872. Re-run the complete 623-case admission audit and account for
   every remaining known gap. Preserve raw one/two-backslash controls, not just
   the examples that currently fail.
5. Require byte/formatter/debug/diagnostic/source preservation, no-handle-leak
   checks and a negative control that a lossy implementation fails. Add required
   package cohorts/omission guards. Run focused then full Rust+Lua, Node,
   Clippy/fmt/Ruff/docs, actual predecessor/fixed archives and corruption controls.
6. Commit/push, verify a clean matching producer and complete clean 4510 replay.
   Ship isolated slices on actual main. Inspect exact-head CI, all five actual
   platform receipts and ten controls before each checked rebase merge. Measure
   repeat/macro memory/runtime against the predecessor and clean temporary work.

Verified existing integration files: `dialects/text.rs`, `ca65_flat.rs`,
`ca65/{mod,project,format}.rs`, `ca65_816.rs`, `ca65_huc6280.rs` and the existing
candidate scripts. `ca65/string_tokens.rs` and dedicated native regression files
would be new. Verify any further call site before adding it to an implementation
slice. The shared AST, public API/schema and dependencies are unchanged.
