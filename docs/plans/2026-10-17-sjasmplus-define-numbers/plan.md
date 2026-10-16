# Numeric DEFINE substitution (#734)

1. Capture SjASMPlus 1.21.0 byte/refusal evidence for identifiers inside existing
   numeric spellings, prefixes/suffixes, separator adjacency, identifier
   boundaries, quoted literals/escapes, AF', chains and recursion. Retain the
   bounded `probe.py` here and facts in
   `crates/asm198x/tests/fixtures/sjasmplus-define-numbers.json`.
2. Add failing native differential, formatter, Pasmo isolation and CLI source /
   five-artifact preservation controls in
   `crates/asm198x/tests/sjasmplus_define_numbers.rs` before modifying production.
3. Correct only the existing DEFINE scanner in
   `crates/asm198x/src/dialects/z80.rs`: scan identifier runs inside numeric
   spellings while keeping complete quoted literals protected. Verify any old
   expectations against native evidence. Keep recursion bounded and preserve
   macro/include contexts. Add the fixture-backed mandatory package case in
   `scripts/test-candidate-package.cjs` and its contract control in
   `scripts/candidate-package.test.cjs`.
4. Run focused then workspace tests, Clippy, formatting, docs, Ruff and Node
   checks. Commit a clean producer, rerun the unchanged pinned SpecNext target,
   retain the report and all raw logs, and check the known #735 DISPLAY boundary.
5. Push a focused PR, verify all final-head checks/workflows plus the actual
   five-host package reports and negative controls before squash merge. Update
   #734, verify remote retention, then remove the owned worktree, source clone,
   Cargo cache, temporary files and task processes.

Task workspace: `Asm198x/asm198x-define-numbers`; owned temporary paths use
`/private/tmp/asm198x-define-numbers-`. Keep the original checkout unchanged.
No new dependency, public schema/API or architectural change is proposed.
