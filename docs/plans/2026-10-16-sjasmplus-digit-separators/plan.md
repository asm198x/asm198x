# SjASMPlus numeric separators (#725)

1. Capture SjASMPlus 1.21.0 byte/refusal evidence across existing numeric forms,
   separator positions, operators, data/STRUCT lists, DEFINE and quoting.
   Retain the executable identity and bounded probe in this directory; store
   captured cases in `crates/asm198x/tests/fixtures/sjasmplus-digit-separators.json`.
2. Add differential integration, formatter and CLI preservation tests in
   `crates/asm198x/tests/sjasmplus_digit_separators.rs`; demonstrate failure first.
3. Correct numeric extent/decoding and affected quote scanners in
   `crates/asm198x/src/dialects/{z80,sjasmplus,macros}.rs`, preserving Pasmo behavior.
   Add the fixture-backed package qualification case in
   `scripts/test-candidate-package.cjs` and its required-case test in
   `scripts/candidate-package.test.cjs`.
4. Run focused then workspace tests, Clippy, formatting, docs and package checks.
   Commit a clean producer, rerun the unchanged pinned SpecNext target through
   the explicit ecosystem runner, retain its result and file any next boundary.
5. Push a focused PR, verify every exact-head check and workflow before squash
   merge, then remove owned temporary worktrees, sources and build caches.
