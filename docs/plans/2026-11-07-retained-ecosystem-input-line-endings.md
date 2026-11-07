# Preserve retained ecosystem evidence inputs on Windows

## Cause and reassessment

Candidate 1083 at a147d93f passed the previous corrections on Windows, then
`ecosystem::execution::tests::focused_captures_cannot_replace_the_published_complete_cohort`
failed at execution.rs:1572. `validate_report` compares the retained manifest
hash to raw `ecosystem/corpus.json` bytes. LF matches retained a853f20b...;
CRLF produces 563b7ced... and correctly fails closed. The reviewed adapter
`ecosystem/adapters/cc65-opcodes.cfg` is also compared by raw length/digest:
LF is 412 bytes/e2ed5423..., CRLF is 418 bytes/100792f6....

The earlier audit covered embedded/generated text and literal-newline consumers,
not the complete raw-read/hash reachability of retained-report validation. That
omission left these byte-identity inputs unprotected. Reassessed the entire
validator (execution.rs:760-1010), manifest_hash and artifact before editing.
Only these two checkout files are raw-byte inputs. Every admitted adapter is
that single cfg file. Other checked identities come from serialized build-plan
values, embedded license text, decoded artifact hex, or structural hash fields
in the parsed report. No additional local license/source file is read there.

## Plan

1. In a verified task worktree, remove the two Git-identical copies and checkout
   with `core.autocrlf=true`. Require actual CRLF bytes, then run the unchanged
   focused Rust test and require stale-manifest rejection.
2. Restore only the manifest's Git LF bytes, leave the adapter CRLF, and rerun:
   require `native evidence has a stale build adapter`. This exposes the masked
   second input before another candidate.
3. Add only two `text eol=lf` rules to `.gitattributes`. Freshly checkout both
   files with the same core.autocrlf setting; require exact Git blobs, then run
   all xtask tests. Keep the manifest/report, adapter and validator unchanged;
   never normalize hashed data or refresh retained evidence.
4. Commit/push and send exact diff and staged controls for parent review before
   merge. Require every applicable current-head PR check. Refresh the candidate
   only after merge and require full native completion including doctests.
5. Verify remote retention and remove task worktree/cache and temporary copies.

## Native gate scope

The existing shared rust-ci.yml@v1 command is `cargo test --workspace`, without
`--no-fail-fast`. Asm does not request the separate all-features input (default
false), so this gate does not claim all-features testing. xtask is the last unit
binary in the actual Windows order; Cargo doctests remain after it. No shared
workflow change is part of this correction.

Evidence: https://github.com/asm198x/asm198x/actions/runs/42406860134/job/128506556591

## Verification

- Both fresh CRLF checkouts: unchanged focused Rust test exited 101 with
  `report is empty, incomplete, stale or has no successful Asm198x producer build`.
- LF manifest only, CRLF adapter retained: the same test exited 101 with
  `native evidence has a stale build adapter`.
- Both LF rules applied, fresh `core.autocrlf=true` checkout: both files equal
  their Git blobs byte for byte. All 80 `cargo test -p xtask` tests passed,
  with no ignored or filtered tests. `git diff --check` passed.
- Scope is four attribute-file lines and this plan; no manifest, adapter,
  retained report, Rust source, assertions or baseline changed.
