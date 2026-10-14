# Pasmo DEFS count normalization

PasmoNext 0.1.3 treats DEFS/DS counts modulo 65536. Asm198x rejects negative counts and allocates the unbounded positive count. Reference probes at origin zero show -1 and 65535 emit 65535 bytes, -65535 emits one, and both signs of 65536 emit none. Output crossing $FFFF is a separate shared placement gap, tracked by #631; this change does not rewrite output placement. Oversized literal/intermediate arithmetic before count normalization is separately tracked by #634 (`dw 65536/2` and `dw (65535+2)/2` already differ from the reference); this patch does not claim arbitrary expression parity.

1. Add bounded count regression tests in `crates/asm198x/src/dialects/pasmo.rs`, and live differential probes in `crates/asm198x/tests/differential.rs`. Prove the tests fail on the current lowering.
2. Normalize Pasmo/PasmoNext counts before allocation in `crates/asm198x/src/dialects/z80.rs`, leaving SjASMPlus negative counts unchanged. Check prior labels and location-counter expressions still agree after normalized counts.
3. Capture live PasmoNext reference verdicts, replay them without external tools, regenerate the corpus ledger, and run workspace tests, formatting and Clippy. Preserve malformed/unknown-count rejections.
4. Submit a reviewed PR after the preceding fill fix, coordinate merge and patch release after 0.0.64 has shipped. Keep #629 open while its crossing-origin reproducer remains blocked by #631.

No dependencies, public schema changes, or general memory-image rewrite.
