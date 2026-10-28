# Share the published PRG encoder

Base: ac925e45 on origin/main. User approved format198x-commodore-c64-prg 0.1
on 2026-10-28 under the umbrella dependency-approval rule.

1. Record existing library and full-file ACME output controls before changing
   serialization. Use bounded original source fixtures outside the source tree.
2. Add the workspace dependency in Cargo.toml and inherit it in
   crates/asm198x/Cargo.toml. Replace only the implementation of
   crates/asm198x/src/prg.rs::prg with PrgFile::encode, preserving the public
   signature, zero-origin fallback and 16-bit truncation. Update the root and
   excluded web-shell Cargo.lock files without unrelated dependency upgrades.
3. Verify existing PRG/CLI tests, full-file byte equality against acme -f cbm,
   and the regular workspace suite with the pinned curriculum checkout. Run
   strict Clippy, formatting, docs checks and the excluded shell lock check.
4. Push a checked PR. Require all current CI and native candidate-package
   qualification to pass, then rebase-merge. Close emu198x/emu198x#1994 only
   after positive consumer evidence; leave asm198x/asm198x#941 open for SNA.
5. Verify retained commits and clean temporary worktrees, cloned fixtures,
   generated PRG files, caches and processes.

No public signature, output format, assembler semantics or SNA writer change
is planned. The published PRG crate has no dependencies. Its shared layout
ownership follows the umbrella formats-graduate decision.

## Scoped verification

The existing PRG library test passes before and after the migration. Four
original source fixtures produce complete files byte-identical to ACME before
and after: $0801 code, mixed data at $C000, one byte at $FFFF, and a zero
origin. The two receipts are identical. Root and excluded-web lockfiles add
only format198x-commodore-c64-prg 0.1.0; no existing package version changes.
The public PRG signature, origin fallback, low-16-bit origin and SNA writer
are unchanged. The pinned curriculum fixture checkout is f11698d0b51b3a6c1209cec3d649229aa35af97f.

A draft PR starts independent native candidate-package qualification while
the broader local workspace suite, Clippy and docs checks run. It must be
ready and all checks complete successfully before merge. Completion evidence
will be linked in its review comments; this plan does not call a pending check
successful.
