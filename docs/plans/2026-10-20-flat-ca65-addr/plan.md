# Restore native 65816 address-word extraction (#909)

Native ca65/ld65 accepts `.addr 65536` in 65816 mode and emits 0000; the flat
reader aliases it to `.word` and rejects it. The existing 254-case sweep shows
ten boundary refusals only in 65816. The reference's DoAddr applies GenWordExpr
before EmitWord in 65816 mode. HuC6280 keeps its complete-value range check.
The binding same-spelling-different-operation decision requires preserving this
semantic difference rather than globally changing Words.

1. Measure 148 CPU/boundary, arithmetic, include/macro/unused, forward, uppercase,
   multi-value and `.word` refusal controls. Retain exact native tools/streams
   and replay the frozen #908 producer before changing code.
2. Give 65816 `.addr` its own declared directive ID and lower it to existing
   low/high expression extraction. Keep word, HuC6280 addr and instruction/data
   range policies intact. Regenerate the affected declared surface in
   docs/book/src/reference/dialects/65816.md and docs/book/search.json through
   cargo xtask docs. This needs
   no new operation or engine mode.
3. Add native-backed permanent CLI and required archive coverage with outputs,
   error locations, formatter bytes/idempotence and debug invariants. Replay the
   broader 254 and preceding #904/#906/#908 cohorts. Keep #910 per-element
   counter gaps separate and lose no preceding native match.
4. Run full checks and frozen local/archive evidence, exact-head CI and actual
   five-host package reports; merge only after qualification and clean task work.
