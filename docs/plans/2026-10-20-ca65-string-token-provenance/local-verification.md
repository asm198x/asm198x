# Byte-string source qualification

The clean branch starts at current main 47fc51643eb98ecdd5ef2f64cfc5035bee52997e.
It preserves the independent npm workflow change already merged there.

The full workspace with Lua passes 1,619 tests, with 38 ignored. All 104 Node
checks pass. Clippy over every workspace target with Lua, rustfmt, Ruff and
generated documentation checks pass. The new three native fixture tests cover
963 cases: 912 acceptances and 51 refusals. Their native byte comparisons,
canonical/idempotent formatting, official Debug198x parsing, symbol/section
round trips and preservation of four previous outputs on refusal all pass.
Quoted authored markers remain literal bytes; injected bare markers are refused.

The private store retains byte values through the source pre-pass. Byte output,
string construction, format width/precision and numeric string inspection use
native bytes. Text argument spelling is retained beside cooked text; other
dialects keep their accessors. The linked reader owns one store; flat expansions
transfer their private store to the walker before parsing. Shared expansion and
numeric Expr remain unchanged. Numeric string-function calls captured inside
braces remain tokens, as four new native controls require.

The retained audits explicitly account for existing gaps: #887 has three raw
malformed literal acceptances, #795 has the scoped numeric query, #872 retains
255 quoted token-list admission differences, #891 retains nine unbraced macro
argument refusals, and #892 retains five computed-quote textual-consumer gaps.
The latter audit also compares diagnostic payloads, avoiding an exit/image-only
false green. No new native mismatch is introduced by the byte-emission slice.

Local archive controls, clean producer identity, fresh 4510 replay and actual
five-host qualification remain required before merge. Exact source snapshots
and logs in this directory distinguish earlier prototypes from that producer.
