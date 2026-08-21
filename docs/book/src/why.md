# Why asm198x

Keep your source and your syntax. Gain tooling that did not exist for them.

## There is nothing to port

Every dialect asm198x reads is somebody else's assembler — ACME, ca65, pasmo,
sjasmplus, lwasm, vasm, RGBDS, and Macro Assembler AS for the rest. Point it at
a file you already build and name the dialect it is written in:

```sh
asm198x --dialect acme main.asm -o main.bin
```

That is the whole adoption cost. No conversion pass, no syntax migration, no
second copy of the source in a house dialect. [Dialects](reference/dialects.md)
lists all 24, and what each one is the syntax of.

## What arrives with it

| Capability | Scope |
|---|---|
| One binary, 20 CPUs | One install for every machine you target, rather than a toolchain per machine |
| `fmt` — canonical layout, idempotent | Z80, 8080, 6800, CDP1802, SC/MP, SM83 and 6809 |
| `disasm`, round-trip verified | 6502 and Z80 |
| `--prg`, `--sna`, `--exe` | The loader format comes out of the assembler, not a later packaging step |
| `--message-format=json` | Structured results and diagnostics, for editors and build scripts |
| `--debug` | A Debug198x sidecar for source-level debugging; flat dialects, plus the ca65 and vasm linked paths |
| `--sym`, `--listing` | A symbol table and an address/bytes/source listing |

The scope column is the point. `fmt` covers seven CPU families and not the
6502; `disasm` reads two. Those are the shipped surfaces, not a roadmap.

## Why trust it with a project that already builds

Because you can check, rather than take it on faith.

**Every CPU is arbitrated against a real assembler.** The differential suites
assemble the same source with the reference tool and compare bytes. Eight tools
across twenty-two instruction sets: ACME, ca65, lwasm, pasmo, sjasmplus, vasm,
RGBDS, and Macro Assembler AS, which covers the fourteen less-travelled ones.

**What they produced is recorded, not remembered.** 5,637 verdicts, each keyed
on the reference tool's own version string, committed to this repository. CI
replays every one of them on machines with none of those tools installed, so a
change that alters our output fails against what the real assembler actually
did — not against a fixture somebody wrote by hand.

**The curriculum assembles byte-identically.** 419 assembly sources from the
Code198x curriculum, across the C64, the NES, the Spectrum and the Amiga, in 617
comparisons. Every one matches the reference tool.

**Where we differ is published.** Nine differences are known and tracked, and
[the list](divergences.md) says what each one is. A tracked difference that
silently stops being a difference fails the build.

That last one is the argument. Parity is a claim you would have to believe; a
list of where we do not match is one you can read before deciding.

## What it will not do

It is not published to crates.io — `cargo install asm198x` will not find it.
Use [the installer or an archive](install.md).

It does not replace your emulator, your linker for platforms that need a real
one, or your build system. It assembles, disassembles and formats.

Ready? [Quickstart](quickstart.md).
