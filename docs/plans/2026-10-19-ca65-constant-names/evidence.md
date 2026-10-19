# Refuse dotted ca65 constant definitions

Native ca65 V2.18 rejects `.bogus=7 / .byte .bogus`; the previous archive
accepted it and emitted `07`. The linked reader reused the shared MOS helper,
which deliberately allows dots for other dialects. No component of a ca65
immutable definition may contain a dot. The ca65-only constant_key check now
rejects those names while preserving authored/qualified names and existing
scope resolution. Other dialects and public contracts are unchanged.

The native fixture retains 21 qualifying cases: ten accepted and eleven
refused. Bounded CLI tests compare native bytes/status, formatting idempotence,
reassembled bytes and section/symbol debug shapes. Included-source refusals
identify the authored file/line in human/JSON output and preserve all five
previous output files and both source files.

The previous real Lua-enabled archive fails only the new constant-name cohort:
it accepts the first native-refused dotted definition. The repaired archive
passes all 49 required cases, including 51 commands in this cohort, and rejects
wrong-byte and corrupt-archive controls. Local compressed reports are preflight
without a producer-lock receipt; five-host CI remains required before merge.

Local verification: 1,528 Rust tests passed (38 ignored); 64 Node tests passed.
Clippy with warnings denied, fmt, Ruff and generated docs passed. After rebasing
onto merged #770/#771, tested crates/scripts/workflows/lockfiles were verified
identical to the tested tree.

Clean producer `a1c1a6bc52a8c5b04ec4bafb88e3bb6184a49234` matches all 564
unchanged native/upstream 4510 bytes, SHA-256
`8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec`.
The focused report retains full comparison bodies and fourteen raw streams;
its verifier checks identity/content/completion and rejects a changed byte.
The full ecosystem ledger remains untouched.

Broader declaration probes are retained separately in deferred-native.json and
the fixture's deferred_cases: native cheap constants are missing (#774),
reserved register/mnemonic names are accepted (#775), and unknown dot tokens in
inactive source escape their native lexical refusal (#776). The cheap-constant
probe records fifteen further native controls for the next slice; none of
those separate gaps is claimed fixed here.
