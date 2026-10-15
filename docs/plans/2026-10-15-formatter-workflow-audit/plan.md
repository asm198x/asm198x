# Formatter and command-mode workflow audit

Audit source b2835c87886f76fdb4686901c7ed1aeb930c826a for #641. Do not change production code in this batch.

1. Read the CLI modes in `crates/asm198x/src/main.rs`, formatter/contract decisions and existing formatter tests. Build a locked CLI in an external target directory.
2. Add a bounded, repeatable Python probe in this folder for valid/invalid formatter sources, command-mode option validation, JSON diagnostics and root-line provenance after `--equ` definitions. Use isolated fixtures, subprocess timeouts, byte-preservation assertions and successful controls. Record exact source/tool/host identities and outcomes twice.
3. Trace reproduced failures to existing code and check open issues before filing bounded GitHub issues. Separate observed symptoms from inferred causes. Retain evidence and record scope limits in a report here.
4. Check the probe and documentation, push an evidence PR, update #641, and remove the temporary worktree/build directory after verifying remote retention. Preserve unrelated checkout state.
