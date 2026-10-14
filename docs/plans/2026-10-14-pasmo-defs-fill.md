# Pasmo DEFS count and fill compatibility

The Spectrum +3 extent diagnostic uses `defs $8040-$,0`. Asm198x 0.0.63 rejects the comma; PasmoNext 0.1.3 accepts it. The common directive parser parses the complete operand as one expression and its constant fold has no live location counter.

1. Record the failing source and reference result in [#628](https://github.com/asm198x/asm198x/issues/628).
2. Add Pasmo and PasmoNext regression tests in `crates/asm198x/src/dialects/pasmo.rs`, plus reference probes in `crates/asm198x/tests/differential.rs`: optional/nonzero fill, location-counter count/fill, preceding constants/labels, truncation and malformed operands. Demonstrate the new tests fail before the fix.
3. Reuse `lower_reserve` in `crates/asm198x/src/dialects/z80.rs` for both Z80 dialects, resolving first-pass counts against the live PC. Preserve SjASMPlus forward/negative-count semantics and Pasmo's existing negative-count rejection.
4. Run live differential tests, recorded replay, formatter round trips, workspace tests, fmt, Clippy and generated-documentation checks. Compare the complete larger diagnostic with Pasmo.
5. Review and land a small PR, then author and check the patch release notes and publish through the existing release workflow. Verify the distributed executable against the reproducer.

Scope excludes changing negative-count compatibility (Pasmo wraps a negative count to 16 bits; this existing mismatch needs its own bounded investigation). No new dependency, public contract or output format.
