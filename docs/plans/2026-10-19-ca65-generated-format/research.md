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
