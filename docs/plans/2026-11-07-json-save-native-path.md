# Compare native SAVE report paths on Windows

Release0.0.77 candidate1073, headf5b307d4, Windows job128455038352 failed
`json_save_artifacts_match_human_files_and_report_resolved_writes` at
`crates/asm198x/tests/cli_json.rs:439`. The successful write note uses
`Path::display()` in `src/main.rs:1528`; the assertion expects a forward-slash
suffix. The test already verifies the actual file bytes and its JSON artifact.
The same Windows job passed both alias regressions and the exact debug sidecar
comparison, so this is a separate test-path portability issue.

1. Show the old suffix assertion rejects an equivalent Windows path; confirm
   normalized comparison accepts both separators but still rejects a wrong
   destination. No production contract change.
2. In `crates/asm198x/tests/cli_json.rs` normalize separators only at that suffix
   assertion and add its original message to failure diagnostics. Review adjacent
   path assertions for the same concrete problem; change no unrelated tests.
3. Run the complete `cli_json` test target, fmt and scoped Clippy, then commit,
   push and create a checked PR. Merge only after current complete checks and
   the scheduled Linux archive qualifications. Refresh the release and verify
   the actual Windows tests and published archives.
4. Remove task worktree and build cache after verifying remote commit retention.

Evidence: https://github.com/asm198x/asm198x/actions/runs/42389809873/job/128455038352

## Verification

The original comparison rejects equivalent `src`, `out` and `exports` Windows
path suffixes; the normalized comparison accepts them and still rejects a
`wrong.bin` destination. Adjacent `cli_json` path checks contain no equivalent
slash-sensitive suffix. All22 `cli_json` tests pass after formatting; scoped
Clippy with `-D warnings`, `cargo fmt --all --check`, and `git diff --check` pass.

The failed Windows job passed the constrained-stack regression at16:13:30 UTC,
the original native alias fixture at16:13:34 UTC, and the Debug198x exact-sidecar
comparison at16:15:45 UTC. This SAVE note assertion failed later at16:18:47 UTC.
