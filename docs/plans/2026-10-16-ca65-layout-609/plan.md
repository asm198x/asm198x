# Active ca65 layout and 0.0.68 checkpoint

1. Reproduce #609 with native ca65/ld65 and the current assembler. Record tool identities and final-byte comparisons, including renamed segments, embedded vectors, pinned overlaps and real area overflow.
2. Replace the hard-coded CODE/VECTORS limit in `crates/asm198x/src/dialects/ca65/link.rs` with validation against active areas and resolved segment extents in `ca65_layout.rs`, invoked with actual lengths from `ca65/mod.rs`. Keep placement in the shared engine. Add regression/native tests under `crates/asm198x/tests/` and retain evidence here.
3. Run targeted regressions, native comparisons, formatting, strict Clippy, Lua-enabled workspace tests and generated documentation checks. Push a focused PR; wait for every exact-head check and workflow before squash-merging.
4. Extend the existing package qualification tests with #634 arithmetic and #609 layout checks. Prepare PR #702's authored 0.0.68 changelog. Verify and merge through the same gates, then inspect actual release candidate and public archive results on all five targets. Retain new release evidence in a separate dated plan folder; do not rewrite previous release evidence.
5. Update the issues with measured outcomes. Inspect and remove task worktrees, caches and scratch files only after verifying retained remote commits and evidence.
