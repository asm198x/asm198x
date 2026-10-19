# Generated labels in canonical formatting (#854)

The frozen canonical producer cafe5cd1d8510679f87607b7cf36d106a11913c0
(CLI SHA-256 371235c2826db37e56ba006a39b5469905d3ba49740e25a15a5df7adff28a25c)
now accepts the five reported sources, but emits their complete declarations at
column 8. `formatter-canonical-style.md` binds labels to column 0. The syntax
reader recognizes static labels only, so it still classifies .ident heads as
operations. Keep the unevaluated expression as source spelling and share the
existing balanced lexical head reader with the selected-end reader; do not
introduce a second expression value or execute text functions during formatting.

Fresh native ca65/ld65 controls: 43 cases, 37 accepted and six refused. The
preceding formatter is idempotent, preserves all native outcomes/bytes and opens
no includes. Fifteen native-valid raw assembly cases fail separately: six
whitespace forms (#848), generated conditional/repeat heads, generated macro
definitions/calls, and static/generated labels on scope/proc openers. Their
native commands and hashed preceding CLI outcomes are retained before changes.
These assembly gaps need distinct issues; formatting must remain independently
safe. The static conditional/repeat and macro controls isolate generated heads.

The 15 raw-assembly gaps are now named explicitly: six whitespace cases (#848),
three generated conditional/repeat heads (#861), two generated macro definition/
call heads (#862), and four labelled scope/proc openers (#863). The formatter
change does not alter those assembly entry points. It emits the unevaluated
label expression at column 0, followed by the operation at the normal column.
Actual native formatting retains all 43 outcomes and all 37 accepted images;
all 37 formatted sources also assemble here with native bytes and debug addresses.

The five #854 exceptions were removed from the 113-case reserved-label fixture.
It now performs all formatter checks, including those generated identifiers,
without expanding their spelling to an ordinary register name. Focused tests
also preserve the prior 115 end-label and .end source-retention controls. Human/
JSON failures in includes preserve five outputs and both sources, with the
original file/line; formatting the root never opens the invalid include.
All 93 Node tests, Lua/all-target Clippy, rustfmt, Ruff and generated docs pass.
Full Rust, clean producer, actual archives and project qualification follow.

An extra delimiter control caught a regression before the source commit:
.ident("answer"):=7 is an address alias, not a colon label. Native emitted 07;
the first generated-head reader split it into a label and an invalid =7 line.
`assignment-boundary-red.json` retains the actual failure; its executable hash
matches the initial archived binary. The shared lexical head reader now excludes
both :: and :=. Three new controls pin ordinary generated = and adjacent/spaced
:=. The final fixture has 46 cases: 40 native accepted and six refused. Native
formatting preserves all 46 outcomes and all 40 images. Only the two := forms
remain unsupported after formatting here, explicitly tracked as #762; the other
38 native-valid formatted inputs assemble with matching bytes/debug addresses.
The 17 raw assembly gaps stay separate and are asserted by issue. Final full
Rust/archive verification supersedes the 43-case preliminary run.

The corrected actual archive passes all 78 cohorts: 184 commands from the 46 new
controls and 344 commands from the now-unconditional 113-case reserved-label
cohort. Changed bytes and corrupt archives fail. All 93 Node tests, focused Rust,
Lua/all-target Clippy, rustfmt, Ruff and generated docs pass with the := guard.
The final full Rust replay and clean producer/project checks are still pending.
