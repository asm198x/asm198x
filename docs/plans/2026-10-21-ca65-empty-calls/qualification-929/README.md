# Checked empty-call source and retained evidence

Source: 9b0daf2efaf8f413792a0c7cee24f2ec9717590d (PR #929).
Checked virtual merge: 6c703436a6390cdb1fd500ed961a91b40cbd051e,
base ee522abd40a0bfa7a61b5581679679690717ccff,
tree 19bafc7ee1021f115df2c7ccdeaeaf38bc61710c.
Candidate workflow: 39630708539. CI: 39630709833.
Cargo.lock SHA-256: 0ee4b612d610b2d581a5b32e1666f431bdfcf55b99cfaed65841aa48e369958a.

All five actual native archives passed 106 mandatory cohorts and both corruption
controls. Each positive report retains 43,265 process invocations, including
2,078 in the new 722-case cohort. The inspector verifies source/tree, producer
lock before/after, archive/executable identities, exact case inventory, every
new process group, native bytes/JSON, formatting and source/macro diagnostics.
All exact-source checks completed before the final PR metadata edit.

Local validation: 1,645 Rust/Lua tests, 38 existing ignored; 115 Node tests;
Clippy/fmt/docs/versions/ecosystem/ledger; clean locked release; fresh 4510 native
564-byte identity with fourteen streams and a changed-byte control. The local
archive passed all 106 cohorts and both controls with artifact-only provenance;
it is not represented as a five-host producer receipt. The checked #925 archive
fails only the newly added empty-call cohort.

The successor baseline retains 39 matrices / 7,309 case rows / 6,700 native
matches, including the follow-up audits on f89cb4c0a02752f44a787d8312b836ae195ca5bc.
These include repeated contexts and known gaps; they are not a completion ratio.
The approved reader preparation is retained at dccdc1b40cf2b9d432efff046eca2fe8eaeb85b9.
#928/#930 remain open. The rejected #922 implementations remain unmerged.

manifest.json maps each original diagnostic path to its SHA-256 and a lossless
compressed blob. Retained baseline roots remain in place; superseded #925
compiled tools/archives are disposable only because their source and checked
receipts are retained. Cleanup rechecks the pushed audit commit, every blob and
current file, complete root inventory, remote recovery refs and process ownership
before deletion. The reused build caches and sole linked reader worktree remain
for #924; no worktree or unique source change is deleted here.
