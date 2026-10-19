# Match native numeric token limits (#788)

1. Re-consult syntax/parse-affecting/record and reference-version decisions.
   Reproduce decimal/hex/binary endpoints with pinned native ca65, including
   arithmetic that reduces to zero, empty-tag counts, active/inactive source,
   textual macros, quoted digits, includes and all ca65 CPU entry points.
2. Identify the scanner boundary in native source and the actual local parser
   seam. Write the private correction design before implementation; avoid an
   arbitrary count or final-expression limit. Keep source phase behavior.
3. Add retained native fixture/integration and required package evidence with
   bytes/status/formatter/debug checks and source/output preservation. Prove the
   preceding executable fails the new cohort before accepting green.
4. Verify isolated Rust/Lua, Clippy, fmt, Node, Ruff/docs, actual local archives
   and failure controls. Push verified commits, retain clean unchanged 4510
   replay, inspect every exact-head CI run and five actual package reports.
5. Merge only the qualified head, verify closure and retained commits, remove
   task worktree/cache and continue remaining confirmed issues.

## Implementation sequence

Land the shared selected-number conversion first: `ca65_flat::parse_number`
serves the linked reader and both flat CPU paths. Retain native decimal's
4294967299 endpoint and leading zeros, reject oversized literal tokens before
arithmetic cancellation or zero-sized tag multiplication, and preserve current
character parsing. Use the 54 arithmetic/constant/tag cases from native.json
plus explicit CPU checks. This stage references #788 without closing it.

The following stage adds lexical source admission for skipped/token-captured
source. It must also cover token-list introspection and text definitions,
exclude quoted/comment digits and unopened includes, and preserve all measured
.end boundaries including operands on its own line. Do not call an expression
parser on skipped source or claim the whole issue is fixed by conversion alone.
The repeat-execution false refusal is tracked separately from numeric limits.
