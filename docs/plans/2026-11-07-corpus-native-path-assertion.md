# Compare native pattern-source paths as paths

## Cause and remaining-target audit

Candidate 1081 at 10a2f181 passed every asm198x integration target and the
asm198x-ir unit tests on Windows, then verdict-corpus's
`patterns::tests::sources_skip_what_a_build_writes` failed. The scanner returns
native PathBuf values. The test converted them to display strings and expected
POSIX-only separators; Windows correctly produced mixed native separators.
This test is about exact source membership and excluding generated build files,
not a serialized path contract. Production scanner paths must remain native.

Audited test-only native-display/string comparisons across the remaining
verdict-corpus, asm198x-ir and xtask targets. This is the only native conversion
feeding a path-string equality. Adjacent SOURCE_TOOLS messages use authored
logical corpus keys; xtask page paths and parsed corpus keys are logical strings,
not native path displays. No other same-mechanism patch is proposed.

## Plan

1. Change only this test in `crates/verdict-corpus/src/patterns.rs`: compare the
   returned Vec<PathBuf> directly with the two expected root.join paths. Retain
   order, exact membership and the exclusion of build/selected.asm and .inc.
2. Meaningful negative control: temporarily let the actual scanner include build
   directories. The corrected assertion must fail with the extra selected.asm;
   restore the scanner unchanged and run all verdict-corpus tests.
3. Run the remaining workspace target tests (`asm198x-ir` and `xtask`) to validate
   the focused audit, plus formatting, scoped Clippy and diff checks. No library
   behavior, baseline, skipped test or gate changes.
4. Commit/push and send exact diff and controls for parent review before merging.
   Require every applicable current-head check and archive qualification, then
   refresh the release and require complete actual Windows/macOS tests again.
5. Verify remote retention, remove the worktree/cache and temporary control file.

Evidence: https://github.com/asm198x/asm198x/actions/runs/42401115071/job/128489090503

## Results

With the corrected assertion, temporarily admitting build/ in the actual scanner
failed the test (exit 101): the left side contained the additional native
`build/selected.asm` path and the right side retained exactly the two sources.
Restoring the scanner passed all 29 verdict-corpus tests. The negative-control
temporary tree was removed explicitly after its expected panic.

Remaining-target verification passed 12 asm198x-ir unit tests, 34 property tests,
5 source-byte tests, 80 xtask tests and 4 doctests: 164 tests total including
verdict-corpus. Formatting, scoped verdict-corpus Clippy with `-D warnings`, and
diff validation passed. Final diff changes only the one test and this plan;
production scanning and native path values remain unchanged.
