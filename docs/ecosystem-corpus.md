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
| 6502Assembly | ACME | 3 | passes | all three targets are byte-identical; cross-file macros (#429), `%` modulo (#455), labels on live macro calls (#457), indented anonymous labels in macro bodies (#458), repeated nested macro-local scopes (#461), and discontiguous origins (#463) are fixed |
| NES-ca65-example | ca65 | 1 | passes | project-specific `TILES` segment/linker config |
| GB ASM Tutorial — Unbricked | RGBDS | 1 | passes | raw 16 KiB artifact matches RGBLINK exactly; `--gb-rom` also matches the 32 KiB artifact finalised by `rgbfix -v -p 0xFF` exactly |
| SpecNext Invaders | SjASMPlus | 1 | passes | `OPT` (#431), `SLDOPT` (#473), unary `~` (#475), `STRUCT`/`ENDS` (#477), forward `DS` counts (#528), the implicit-accumulator `add (hl)` spelling (#533), STRUCT initialiser lists (#548), column-0 labels that spell a mnemonic or directive (#551), initialiser lists across lines with nested `{ … }` groups (#552) and macros defined in one file and invoked from another (#557) are fixed; the next boundary is a temporary label (`jr nc,1F` at `src/music.asm:175`) |
| HelloAmi | vasm | 1 | passes | re-audit after the unsigned condition-code alias family; no known source rejection remains |

This is already useful evidence: Unbricked now passes Asm198x unchanged at both
the linked-bank and final-ROM boundaries, while the remaining earliest failures
exercise four different parts of the compatibility surface. The corpus is
therefore a backlog generator as well as a regression suite.

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
