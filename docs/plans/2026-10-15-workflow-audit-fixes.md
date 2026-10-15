# Fix the first workflow-audit findings

## Scope and approach

Fix #643, #644 and #645 as separate verified commits/PRs, preserving the existing CLI, dialect and diagnostic models. No dependencies or public schema changes. #643 preflights predictable destination failures before any assembly output write; this is not a cross-file transaction or a guarantee against filesystem races. #644 uses the existing diagnostic array for CLI I/O errors. #645 attaches the active statement's existing source/provenance to device validation errors.

## Tasks

1. #643: add failing process-level regressions in `crates/asm198x/tests/cli_output_safety.rs` for existing/absent primary outputs, both reporting modes, explicit/source-named symbols and SAVE/sidecar destinations. Verify valid destinations still emit the same bytes. Add shared destination preflight in `crates/asm198x/src/main.rs` immediately before output writes, using the existing path-resolution and overwrite rules. Run focused CLI tests, workspace tests, formatting, Clippy and generated-doc checks; push a PR and squash-merge only after its required checks pass.
2. #644: reproduce missing input/config/include and failed primary/SAVE/sidecar destinations in `crates/asm198x/tests/cli_json.rs`. Route unreported I/O failures through the existing `Diagnostic` JSON array, preserving human mode and avoiding double reporting. Change `crates/asm198x/src/main.rs` only as required. Verify success/rejection controls, valid JSON/empty stderr and nonzero exits; run the same relevant gates and land separately.
3. #645: add root/include/macro PAGE/SLOT range-error regressions in `crates/asm198x/tests/multifile.rs` and CLI coverage where appropriate. Fix the device-error call sites in `crates/asm198x/src/engine.rs` so line-zero device errors gain the statement's location without changing global span-less errors. Preserve macro frames. Run focused tests and normal gates; land separately.
4. After the fixes and the separately owned Emu #1775 work, rerun the recorded reproductions and audit a new bounded group: output collisions/aliases, map/listing parity between human/JSON, nested include/macro location and browser media/state recovery. File verified new findings or link existing issues. Keep the broad audit parents open for untested hosts and remaining acceptance.

## Evidence and cleanup

Keep bounded command logs in the task scratch directory while working; record relevant outcomes in PRs/issues. Demonstrate each regression can fail before relying on its green result. Preserve existing work in the original checkouts. Reuse this worktree across the three fixes, then remove it and its isolated build cache after verifying commits on the remote.
