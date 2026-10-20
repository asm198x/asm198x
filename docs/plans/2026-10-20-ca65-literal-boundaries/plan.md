# Native literal boundaries (#887, #893)

Fix the remaining selected-source false acceptances on the approved byte-token
architecture. The retained 354-case native audit differs in 78 cases: 56 malformed
literal operands, four numeric `.asciiz` operands and 18 inactive/captured lexical
cases under #835. The first two groups are this slice. The root cause is stripping
only the first/last quote, and sharing numeric `.byte` parsing with `.asciiz`.

1. Retain the complete original native audit and reproduce against the frozen
   #894 producer. Run additional flat native audits using `.setcpu` for 65816 and
   HuC6280. Keep all 18 excluded lexical cases per CPU explicitly under #835.
2. In `crates/asm198x/src/dialects/ca65/string_tokens.rs`, validate one complete
   physical literal in `data` and `bytes`. Resolve computed handles first;
   backslashes are literal and a computed quote remains valid. Preserve token
   list spelling and existing lazy reading.
3. In `ca65/mod.rs`, `ca65_816.rs`, and `ca65_huc6280.rs`, require string items
   for `.asciiz`; retain one final NUL for comma-separated strings. Leave numeric
   `.byte` operands intact. No Expr, schema, dependency or lexical-policy change.
4. Extend `tests/ca65_string_bytes.rs` and its fixtures with complete native
   coverage accounting, accepted byte/format/debug invariants and four-output
   preservation on refusal. Extend `scripts/test-candidate-package.cjs` and its
   omission tests with a required literal-boundary cohort. Verify the old producer
   fails that cohort and the new executable passes it and all existing cohorts.
5. Run workspace/Lua, Clippy, rustfmt, Ruff, Node and docs checks; freeze a clean
   producer and fresh 4510 replay. Push conventional commits, inspect all required
   CI plus five actual candidate archives and ten corruption controls, then
   rebase-merge after #894. Preserve remote evidence and remove owned worktrees.

The expanded flat audit exposes the existing structural gap #895: 264 flat
selected/inactive/repeat cases are excluded from this slice (198 native accepted
false refusals). Another 30 cases are excluded under #835. The required cohort
therefore executes 768 cases, 477 accepted and 291 refused. All accepted images
and debug output are checked; 61 HuC6280 macro cases retain the known formatter
exception #821. The other 416 accepted cases run both format passes and reassembly.
Complete native audit coverage is 1,062 cases; none is silently dropped.

The flat formatter previously passed `.asciiz .sprintf(...)` through numeric
placeholder parsing. Tightening assembly types exposed that accidental coupling.
Its existing source-only node path now retains `.asciiz` syntax without evaluating
constructors; canonical placement, labels and spelling survive. The complete
existing byte-string cohorts test byte-identical/idempotent reassembly.
