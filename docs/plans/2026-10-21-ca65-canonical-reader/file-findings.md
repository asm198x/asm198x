# Linked file-field migration checkpoint

Include/incbin request values and optional binary windows now use the canonical
input. The existing private selected-line callback distinguishes a statement
Kind from a WalkDirective after ordinary label binding. File parsing produces
a request only: resolve_at_sweep, loader/path/window checks, pending include
priority, source trivia and operand/macro spans retain their existing owners.
Pure formatter file parsing is untouched and opens no targets.

All 200 file audit controls match native, up from 137. They pass complete CLI
contracts and an independent memory-loader pass, including refusals, byte/debug
identity, idempotent formatting, scoped/forward/cyclic values, labels, zero/end/
negative/large windows and malformed tails. The 39-matrix / 7,309-row unchanged
executable replay retains all 6,865 predecessor matches and reaches 6,934.
Numeric-consumer controls reach 50/50; scalar/canonical/generated/name controls
remain 320/320, 100/100, 40/40 and 18/18 respectively.

Verification passes 779 library tests and 50 affected source/deferred,
include-stream, formatter, captured-text, text-consumer/call and output-protection
contract tests. Clippy -D warnings and formatting pass. Exact lossless receipts
are retained in file-checks/. The full reader remains unmerged: other name/head/
statement adapters, flat readers, canonical syntax-only reading and final
current full workspace/archive qualification still require completion.
