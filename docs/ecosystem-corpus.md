# The ecosystem source corpus

The verdict corpus asks whether generated source produces the bytes a reference
assembler produced. The ecosystem corpus asks the broader question: **does an
unrelated, real project assemble unchanged?**

The index is [`ecosystem/corpus.json`](../ecosystem/corpus.json). Validate and
inspect it without network access:

```sh
cargo xtask ecosystem check
cargo xtask ecosystem list
```

Fetch every pinned tree, or one named project, into a directory outside this
repository:

```sh
cargo xtask ecosystem fetch /tmp/asm198x-ecosystem
cargo xtask ecosystem fetch /tmp/asm198x-ecosystem kindjie-6502assembly
cargo xtask ecosystem verify /tmp/asm198x-ecosystem
```

Fetch is intentionally inert: it clones and checks out the exact commit, then
checks that the recorded licence exists. It never runs a Makefile, script, or
assembler from the downloaded project.

## Admission rules

A project enters only when all of these are known:

1. It is independently authored source intended for a dialect Asm198x claims.
2. A full commit pins the observation.
3. Its licence explicitly permits the intended use, and the manifest points to
   the grant.
4. The native assembler invocation and build boundary are recoverable.
5. The source is kept unchanged and complete for that target.
6. The native control builds with the declared reference version. If a modern
   tool rejects old source, establish the historical tool first; that is a
   tool-generation finding, not an Asm198x result.

Build helpers and post-processors are recorded only where they affect the
artifact boundary. A failure in `rgbfix`, an image converter, an emulator, or a
disk packer is not an assembler rejection.

## Initial audit — 2026-08-30

The five admitted projects contain seven build targets across five dialects.
Their native controls pass with the installed verdict-corpus arbiters: ACME
0.97, ca65 2.18, SjASMPlus 1.21.0, vasm 2.0b, and RGBASM 1.0.3.

| Project | Dialect | Targets | Native control | First Asm198x boundary |
|---|---:|---:|---|---|
| 6502Assembly | ACME | 3 | passes | macro defined by preceding `!source` is not visible at `+start_at` |
| NES-ca65-example | ca65 | 1 | passes | project-specific `TILES` segment/linker config |
| GB ASM Tutorial — Unbricked | RGBDS | 1 | passes | operandless `db` storage declarations in WRAM at `wFrameCounter: db` after graphics literals were fixed in #439 |
| SpecNext Invaders | SjASMPlus | 1 | passes | `OPT` directive |
| HelloAmi | vasm | 1 | passes | `BLO` condition-code spelling |

This is already useful evidence: none of the first seven independent targets
passes Asm198x unchanged, and the five earliest failures exercise five different
parts of the compatibility surface. The corpus is therefore a backlog
generator as well as a regression suite.

The table records the first boundary, not the only one. Once a boundary is
implemented, rerun the unchanged source and let the next one surface. A target
becomes a byte-identity regression case only after it reaches successful
assembly and its native and Asm198x artifact boundaries are equivalent.

## Adding a project

Add a manifest row, using paths relative to the project checkout. Commands are
JSON arrays: one executable followed by its exact arguments, never a shell
string. Then run:

```sh
cargo xtask ecosystem check
cargo xtask ecosystem fetch /tmp/asm198x-ecosystem <project-id>
```

Review the upstream licence and build instructions at the pinned commit, run
the native control deliberately, and record the first Asm198x boundary in this
page until the automated audit ledger supersedes the bootstrap table.
