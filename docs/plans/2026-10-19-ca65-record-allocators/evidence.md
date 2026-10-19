# Validate ca65 record allocations (#786)

The record reader now requires a `.res` size, a positive instance count and an
allocated field size no greater than 0xFFFFFF. Multiplication is checked before
symbols or offsets change. This follows native results: an empty tag accepts a
positive count above 32 bits, so the check bounds allocation, not count.
Formatter reading preserves authored inactive declarations.

The fixture retains 386 native cases: 230 accepted and 156 refused. It covers
structs and unions, named and anonymous fields, all nine existing allocators,
missing/nonpositive counts, paired allocation boundaries, empty tags and dead
controls. The predecessor disagrees on 132 of the initial 380 cases. The final
six controls include positive empty-tag counts of 4294967296.

Two Rust tests compare native bytes/status, formatter idempotence/reassembly
and debug shape, and verify included negative-field diagnostics preserve all
five output files. Isolated verification passes 1,543 Rust tests (38 ignored),
70 Node tests, Clippy with all features, fmt, Ruff and generated docs. A real
Lua-enabled archive passes all 55 required cohorts and both negative controls;
the preceding archive fails only this new cohort, which runs 1,076 commands.
These local archives have artifact-only receipts, not CI producer receipts.

Before merge, preserve the tested source across parent rebases, replay the
unchanged 564-byte 4510 fixture from a clean producer, and inspect exact-head
CI plus all five actual package reports and ten negative controls.

The separate numeric-literal refusal gap is retained in
`deferred-huge-literal-native.json` and tracked as #788. No public shape or
dependency changed.
