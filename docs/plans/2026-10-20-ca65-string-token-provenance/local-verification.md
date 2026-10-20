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

The clean producer f920ac1adb4c11572908107cbde53a83b17b428a has tree
297bb22a53d0fcd2a4490d0c9b5344da4be5d9e0 and CLI SHA-256
31dd8c0220a4d6efef8e1b3114da99ac3baf9be7b44fc09b00f908551e45dda2.
The version result and exact source hashes are retained in clean-producer.json.
All complete native audits reproduce the counts above with this executable.

Its local archive passes all 89 required cohorts. The three new cohorts execute
1,109, 372 and 2,218 commands. Packaging the retained main executable fails
exactly those three new cohorts. The wrong-byte control fails exactly
reference_bytes/determinism/structured_success; corrupted archives fail SHA-256
validation. Their complete reports are retained.

The fresh 4510 replay matches all 564 native/upstream bytes, retains all seven
processes and fourteen streams, and rejects a changed-byte control. The driver
removed its disposable work directory; that removal was verified. Source pin
and build identity remain in the retained complete replay report. Exact-head CI
and all five actual packages with ten controls remain required before merge.
