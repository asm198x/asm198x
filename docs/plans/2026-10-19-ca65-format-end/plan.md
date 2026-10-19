# Canonical ca65 syntax reader (#833)

The user selected design B on 2026-10-19. Keep canonical layout and operand
spelling. The semantic reader currently executes `.end` during formatting;
26 retained native controls include zero-repeat and unused-macro source loss,
called macros, unread oversized tokens and unclosed blocks, comments/quotes,
labels, text aliases, constant runs and invalid capture input.

1. Inspect `ca65::parse_program`, `Walker::parse_line`, the shared block cursor,
   `ast::emit_nodes` and `equ_run_widths`. Retain native tool identities and
   clean predecessor formatting in `native.json` and `predecessor.json`.
2. Add a private source syntax reader in
   `crates/asm198x/src/dialects/ca65/format.rs`, called only for `Expand::No`.
   Read every physical line without executing directives, expanding macros,
   resolving includes, validating numbers, or requiring closed semantic blocks.
   Classify comments, labels, assignments and source statements lexically.
   Preserve operand tokens and unread tails. Do not use whole-file Verbatim.
3. Reuse the shared AST canonical renderer for labels, indentation, comments and
   constant runs. If a formatting-only assignment marker is needed, keep it
   crate-private and make semantic lowering reject it; never fabricate a
   numeric expression to stand in for unread source. No public API/schema or
   dependency change. Assembly retains its existing reader and diagnostics.
4. Add `crates/asm198x/tests/ca65_format_end.rs` and a retained fixture. Assert
   canonical layout, token/source completeness, idempotence, native formatted
   acceptance and byte equivalence. Distinguish existing assembly gaps #830
   and #844 from formatting behavior. Invalid capture tokens must retain their
   existing assembly diagnostic and five-output preservation.
5. Require `ca65_format_end` in `scripts/test-candidate-package.cjs` and add an
   omission control in `scripts/candidate-package.test.cjs`. Run focused/full
   Rust+Lua, Node, Clippy, fmt/Ruff/docs, actual preceding/current archives and
   corruption controls. Exercise existing formatter cohorts to detect layout
   and semantic regressions beyond the `.end` fixture.
6. Ship only this task's commits on current main, retaining a clean affected
   project replay and full bytes/streams. Inspect exact-head CI and five actual
   package reports, source/lock receipts and ten controls before rebase-merge.
   Verify remote retention before removing finished owned workspaces/processes.
