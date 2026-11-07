# Preserve Debug198x fixture bytes on Windows

The 0.0.77 candidate Windows job128448922144 (run42387773814, headc894b1bc)
passed both `long_forward_aliases_fit_a_small_stack_and_still_refuse_cycles`
and `native_deferred_aliases_resolve_without_changing_source_time_forms`.
It then failed `the_walk_describes_its_link_as_the_old_reader_does` at
`crates/asm198x/tests/ca65_lang198x.rs:338`: generated LF sidecar bytes differed
from the CRLF checkout of the committed 6502-nes sidecar. JSON content matched.

1. Reproduce checkout conversion in a disposable index with `core.autocrlf=true`.
   Verify the real `tests/fixtures/debug198x/*.debug198x` paths differ from Git
   blob bytes before the fix.
2. Add only an LF attribute for those exact-output text fixtures in
   `.gitattributes`, following existing retained-byte fixture rules. Repeat the
   same checkout and require exact equality with Git blobs. Keep sidecar output
   assertions unchanged; no runtime or fixture-content edits.
3. Commit and push a checked PR, wait for complete current CI and both Linux
   archive qualifications, then rebase merge and refresh the release candidate.
   Actual Windows tests must pass before release. Remove disposable checkout,
   index, and task worktree after preserving the commit remotely.

## Verification

The disposable `git read-tree HEAD` / `git -c core.autocrlf=true checkout-index`
control changed all nine real sidecars from committed LF to CRLF before the
attribute. With the attribute, the identical checkout preserved every Git blob
byte. `git check-attr text eol` confirmed `text: set` and `eol: lf` for all nine.
`git diff --check` passed. No Rust code or fixture content changed.

Windows evidence: the constrained-stack test passed at15:48:26 UTC and the
original native deferred-alias fixture passed at15:48:33 UTC in
https://github.com/asm198x/asm198x/actions/runs/42387773814/job/128448922144.
The later sidecar comparison failed at15:50:49 UTC.
