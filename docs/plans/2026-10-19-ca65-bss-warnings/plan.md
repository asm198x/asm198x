# Warn for initialized content in uninitialized segments (#807)

1. Re-consult syntax, non-byte verification, diagnostics and layout decisions.
   Retain native ca65/ld65 source/config/process outcomes for types zp/bss/ro/rw,
   file/non-file areas, data, instructions, explicit/uninitialized reservations,
   alignment, zero lengths, includes and multiple affected segments. Count the
   native initialization warnings and compare the explicit #801 producer.
2. Write the concrete private-path design after identifying native triggers.
   Preserve original statement initialization intent before resolution erases
   optional fill, use the existing advisory warning shape, warn once per affected
   segment and preserve source location. Do not change bytes or public schemas.
3. Add focused integration tests for native bytes/status/warning counts, human
   and JSON diagnostics, included-file provenance and refusal output preservation.
   Add required package cohort and an omission control in the existing scripts.
4. Run isolated Rust/Lua, Clippy, fmt, Node, Ruff, docs and actual archives. Prove
   the preceding actual archive fails the new cohort. Retain a clean unchanged
   4510 producer replay, push verified commits and open a conventional PR.
5. Inspect all exact-head workflows plus five actual package reports and their
   ten corruption controls. Squash after #801, verify retention and closure,
   clean the finished workspace/cache and continue the next confirmed issue.
