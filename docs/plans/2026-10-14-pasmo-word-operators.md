# Pasmo word-form bitwise expressions

Issue [#632](https://github.com/asm198x/asm198x/issues/632) exposes a lexer gap: `XOR` becomes a symbol, although PasmoNext 0.1.3 accepts it. The reference also accepts `AND`, `OR`, `NOT`, `SHL` and `SHR`. Its precedence differs from SjASMPlus: OR/XOR share one left-associative level, comparisons bind before bitwise operations, and shifts share multiplication's level.

1. Probe these spellings and adjacent precedence levels against local PasmoNext 0.1.3; preserve accepted and rejected reference verdicts through the existing differential/replay infrastructure. The [upstream operator table](https://pasmo.speccy.org/pasmodoc.html#operators) provides the source contract.
2. Add failing regression cases in `crates/asm198x/src/dialects/pasmo.rs` and live probes in `crates/asm198x/tests/differential.rs`, including the DEFS reproducer, mixed case, operator boundaries, precedence, conditions, and SjASMPlus separation.
3. Add a Pasmo expression-syntax hook and use existing tokens, AST and evaluation in `crates/asm198x/src/dialects/z80.rs`. Leave SjASMPlus's parser unchanged. Do not generalise the shared expression architecture or add dependencies.
4. Run live differential, replay, formatter round trips, workspace tests, formatting, Clippy and generated-documentation checks; retain the exact reference results.
5. Open a reviewable PR. Coordinate landing and the next patch release with the parent session; do not race the in-flight 0.0.64 release.

This step covers the six bitwise/shift word spellings and their grouping with already supported arithmetic/comparison tokens. Other Pasmo operators and 16-bit intermediate-value overflow behaviour remain outside this fix.
