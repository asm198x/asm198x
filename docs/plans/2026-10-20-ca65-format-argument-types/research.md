# Numeric format typing is already strict; string bytes are not preserved

The native 71-case audit accepts 31 inputs and refuses 40. Frozen clean producer
3518aaa314a7b6632b545c1f75d047cf14fddc86 has eight differences. The numeric
.sprintf hypothesis is disproved: all direct numeric/string conversion type
controls match. format_one already rejects Arg::Text for numeric conversions
and requires it for %s. Do not apply #884's selector correction here.

Four constructed-string cases emit wrong bytes. .sprintf("%c",34) should emit
one quote byte, but emits backslash/quote. Character 92 should emit one
backslash, but emits two. A single-backslash .concat input and %s string input
also acquire an extra backslash. The shared text engine escapes computed text
as C-style quoted source; ca65's byte parser preserves literal backslashes.
The two-backslash controls happen to agree because unescaping and re-escaping
cancel. A global decode/encode toggle would change other cases incorrectly.

Three malformed raw .byte inputs are falsely accepted: an escaped quote,
adjacent quoted literals and quoted strings separated by +. The imported MOS
string_literal helper checks only first/last quote and returns all interior
text as bytes. Native rejects these complete data operands. Literal validation
must retain token boundaries and distinguish authored source from a computed
string value; treating both as a reconstructed quoted string loses that fact.

The eighth difference is the existing #795 scoped numeric visibility control:
VALUE=2 inside .scope is valid for native .sprintf but not found by the current
text numeric evaluator. Keep it separate from byte-string representation.

Retain native commands/identities, all complete byte images and preceding
process outcomes. File the generated-string byte corruption and malformed raw
data admission separately, then write a concrete representation design before
changing a source/pre-pass boundary. The source AST formatter and shared Expr
remain unchanged; new architecture needs agreement under the supplied AGENTS.

## Full byte and UTF-8 precision sweep

The additional 287 cases accept 274 natively and refuse 13. The preceding
producer differs in 145. All values 1..255 are tested through %c; zero remains
refused by both. Values 128..255 are emitted as two UTF-8 bytes here instead of
one native byte. Quote/backslash values gain an escape byte. Native %.1s cuts
UTF-8 input at a byte boundary (é becomes c3); the current implementation keeps
the complete Unicode character (c3 a9). Nested concat/%s and asciiz retain these
differences. Native and current code both refuse string values in .word/.dword
and immediate operands; preserve those controls.

This is a byte-string representation problem, not a reason to loosen numeric
format typing or globally unescape data. A proposed private ca65 string-token
layer must retain raw argument spelling, owned byte values and token identity
through the shared text/macro mechanisms, materialize data directly, and never
expose internal handles in source/diagnostic/debug/listing contracts.
