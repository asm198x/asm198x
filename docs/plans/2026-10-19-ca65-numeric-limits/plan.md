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
