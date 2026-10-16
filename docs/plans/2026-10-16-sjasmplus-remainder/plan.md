# SjASMPlus remainder (#731)

1. Capture bounded SjASMPlus 1.21.0 executions covering `%`, `MOD`/`mod`,
   signed values, precedence/associativity, 32-bit intermediate wrapping,
   binary-prefix adjacency, symbols, IF/ASSERT/data/EQU and zero divisors.
   Retain `probe.py` here and the replay fixture in
   `crates/asm198x/tests/fixtures/sjasmplus-remainder.json`.
2. Add failing native differential, formatter, Pasmo isolation and CLI
   include-diagnostic/output-preservation tests in
   `crates/asm198x/tests/sjasmplus_remainder.rs` before changing production code.
3. Extend the existing private expression tokenizer and multiplicative parser
   in `crates/asm198x/src/dialects/z80.rs`, using the neutral engine's existing
   `BinOp::Mod`. Keep binary literals and MOD symbols readable in value position.
   Add the mandatory fixture-backed case to `scripts/test-candidate-package.cjs`
   and its contract check to `scripts/candidate-package.test.cjs`.
4. Run workspace tests, Clippy, formatting, generated docs, Ruff and Node
   checks. Commit a clean producer and run the unchanged pinned SpecNext target
   through the explicit ecosystem runner. Retain the full report/logs and either
   an exact NEX match or a separately filed next boundary.
5. Push a focused PR, read every final-head check/workflow and actual five-host
   package reports before squash merge. Preserve evidence, then remove owned
   worktree, cache, source acquisitions and processes.

Task worktree: `Asm198x/asm198x-remainder`; all temporary paths use
`/private/tmp/asm198x-remainder-`. The original checkout stays unchanged.
No dependency, public API/schema or architectural change is needed.
