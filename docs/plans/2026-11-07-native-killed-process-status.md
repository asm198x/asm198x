# Keep killed reference tools outside source verdicts on every host

## Reassessment and cause

Release candidate 1075 at 3c2c5b9c passed the prior alias, sidecar and SAVE fixes
on Windows. The later `a_tool_killed_by_a_signal_is_not_a_verdict` assertion at
`crates/asm198x/tests/conformance.rs:2285` failed. Git-for-Windows `sh -c
'kill -9 $$'` finishes with native process exit 2304, so `ref_outcome` correctly
returns `NonVerdict { reason: "the tool exited 2304 without saying why" }`.
Unix reports no exit code for the killed process and the same classifier
correctly describes a signal. The assertion assumed Unix wording on every host.

The release's repeated failures have been distinct portability defects, rather
than failed attempts to repair alias evaluation. Reassessed before editing;
parent explicitly authorized this bounded correction and requested review
before merge. No runtime behavior, test skip, stack change or gate weakening.

## Plan

1. Inspect only signal/raw-process-status tests for the same assumption.
2. Update this assertion in `crates/asm198x/tests/conformance.rs` to require
   `NonVerdict` on every host and the appropriate native explanation: signal
   on Unix, silent nonzero process exit on Windows. Keep the real killed-shell
   command and meaningful failure output.
3. Negative control: temporarily substitute a diagnostic source refusal for the
   killed command. The corrected test must fail because the result is a verdict;
   restore the command and run the full nonignored conformance target, fmt and
   scoped Clippy. Record actual Windows red evidence and local green results.
4. Commit, push and send the checked PR for parent review before merging. Require
   complete current checks and Linux archive qualifications. Refresh the release
   and require complete current native tests and end-chain artifact qualification.
5. Verify the remote retains the exact commit, then remove task worktree/cache.

Evidence: https://github.com/asm198x/asm198x/actions/runs/42392095306/job/128461803980

## Results

The focused audit found no other killed-shell, raw-status construction or
signal-reason assertion in `crates/asm198x/tests`.
The diagnostic-refusal negative control failed the corrected test with
`Rejected { diagnostic: "source-refusal" }` (exit 101). Restoring the actual
killed command passed all 25 nonignored conformance tests; the 14 existing native
reference-tool tests remain ignored as before. Formatting, scoped Clippy with
`-D warnings`, and `git diff --check` passed. No new tests are skipped.

The actual Windows red result is retained in the linked job. The platform's
silent-exit explanation is accepted only together with `NonVerdict`; a real
source refusal continues to be a `Rejected` verdict and fails this assertion.
