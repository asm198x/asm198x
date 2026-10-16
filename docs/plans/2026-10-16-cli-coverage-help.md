# Align CLI coverage descriptions (#679)

The reference describes ordinary disassembly as 6502 or Z80, while dispatch
already selects other implemented decoders. Help's formatter line carries an
older hardcoded subset and omits working ACME formatting. Correct these descriptions
without adding another CPU ledger; point to the existing dialect/target guidance.
Keep ordinary listings distinct from opt-in, base-6502 recovery.

1. Inspect help/reference and implemented dispatch. Edit only the coverage wording
   in `crates/asm198x/src/main.rs` and `docs/book/src/reference/cli.md`, including
   the directly corresponding stale dispatch comment. Show concrete 68000
   disassembly and ACME formatting invocations using existing supported options.
2. Build the CLI and execute both examples in a task-owned scratch directory.
   Verify a position-dependent 68000 branch against the decoder and reassemble
   the ACME formatted result to the original bytes. Check help output, existing
   CLI/disassembly/formatting/recovery tests and generated documentation. This is
   a prose correction; do not add tests that pin wording.
3. Push a focused conventional-commit PR closing #679. Squash merge only after
   exact-head checks and triggered workflows finish acceptably. Update #679 and
   #641 with the result; inspect and remove the task worktree/build/scratch files
   after verifying remote retention.

Verification results will be retained below.

## Verification

- Before editing, the built CLI reproduced the stale formatter list while both
  described operations already worked. The cause is handwritten coverage prose
  left behind after the existing dispatch expanded, not missing functionality.
- The final help uses generic dialect/target placeholders and the existing
  dialect table. The reference describes decoder selection, defaults and the
  separate opt-in recovery workflow without promising new output syntax or
  machine coverage. No dispatch behavior changes.
- Both exact documented commands were executed with the rebuilt binary. Bytes
  `60 02` at `$10000` produce `bra.s $10004`; the corresponding decoder-agreement
  regression also passes. The ACME `fill.a` example formats to `formatted.a` and
  reassembles to the original `A9 51 60`. The original source remains unchanged.
- All 80 selected dialect, disassembly, formatter and recovery tests pass; one
  external-reference test remains intentionally ignored. Workspace Clippy with
  `-D warnings`, rustfmt and diff checks pass. No wording-pinning tests added.
- The first generated-doc check failed with `docs/book/search.json is stale.`
  Running the existing generator updates that file; only the CLI reference's
  search entry changes. The final generated-doc check passes: nine blocks across
  64 pages and 47 generated pages current, with all 63 navigation pages present.

The generated `docs/book/search.json` belongs to the same correction and is
included alongside its reference source. Task files use an isolated TMPDIR.
The help wording reaches users with the next normal CLI release; this does not
rewrite the historical 0.0.66 audit or cut a release for prose alone.
