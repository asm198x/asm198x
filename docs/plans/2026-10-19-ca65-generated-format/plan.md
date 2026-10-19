# Qualify generated identifier formatting (#854)

The shipped canonical reader (#859/#833) retains generated-label syntax without
executing it. The existing 113-case reserved-label fixture still marks five
native-valid generated A/X/Y/Z/F forms as formatter gaps. Reproduce current
native and preceding formatter behavior before retiring those exceptions.

1. Re-read canonical-formatting, text-layer and lazy-reading decisions. Retain
   fresh ca65/ld65 controls for the five reported forms, operand spelling and
   comments, expression/whitespace variants, scoped/cheap contexts, block/macro
   heads, includes and refused forms. Compare native bytes and current frozen
   formatter idempotence/native reassembly. Keep assembly gaps separate from
   formatter behavior and file newly reproduced gaps as GitHub issues.
2. Native-valid computed labels now retain meaning, but the frozen canonical
   predecessor indents `.ident(...)` as an operation. The binding canonical
   decision requires labels at column 0. Share the existing balanced generated-
   head recognizer from `ca65/project.rs` with the syntax reader through a
   private helper beside `split_label_head` in `ca65/mod.rs`. Read syntax only;
   keep the authored expression as the AST symbol's source spelling and use the
   existing emitter. Do not evaluate or rewrite the generated name. Remove the
   five fixed `formatter_gap` fields in
   `crates/asm198x/tests/fixtures/ca65-reserved-labels.json`. Run
   `crates/asm198x/tests/ca65_reserved_labels.rs`; extend coverage in a dedicated
   fixture/test/cohort only where the extra contexts add an independent control.
   This stays within the agreed canonical reader; no dependency or public API/
   schema change. Re-run the existing 115 end-label controls after extraction.
3. Verify byte/address equality, complete source/comments and idempotence with
   actual native commands. Retain any new refusals and previous-output/source
   preservation. Run the applicable Rust/Node, Clippy, fmt/Ruff/docs checks and
   actual archives, proving old binaries fail if any new behavior is required.
4. Ship only own test/evidence changes on current main after #860. Inspect all
   exact-head checks, five actual packages/receipts and ten controls; then
   rebase-merge, close only supported scope and clean finished owned workspaces.
