# Preparation on checked #929

The v2 preparation history starts from merged 71ee7240a8518c2deadd4340d9e10d0dd6bd33d9.
The numeric-parser conflict preserves the borrowed peek/advance seam and #929's
empty-vector call grammar. No pushed history was rewritten.

Both library checkpoints passed 777 tests. The 722-case empty-call fixture
passed exact statuses/bytes, JSON, diagnostics, frames, debug-reader and formatting
round trips. All-target Clippy, fmt and docs passed. The immutable #929 baseline
contains 39 matrices / 7,309 case rows / 6,700 native matches; every new debug CLI
exit code, byte/output-retention result and native-match verdict is identical.
The replay asserts complete row/name inventories and an unchanged executable
hash throughout. No semantic consumer is yet migrated to the new token stream.
This remains unmerged preparation for #924, not completed canonical parsing.

manifest.json retains the exact temporary verification paths, file hashes and
lossless blobs before deletion. The old feature and audit refs remain pushed;
the current task worktree remains for the approved migration.
