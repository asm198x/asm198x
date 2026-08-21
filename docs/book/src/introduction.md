# asm198x

Assemblers and disassemblers for retro CPUs. One binary, no runtime
dependencies, the same interface on macOS, Linux and Windows.

```sh
asm198x prog.asm -o prog.bin           # assemble
asm198x disasm prog.bin                # disassemble to stdout
asm198x fmt prog.asm                   # reformat, to stdout
```

## Where to start

| If you are | Start at |
|---|---|
| Deciding whether this is worth your time | [Why asm198x](why.md) |
| Ready to run something | [Quickstart](quickstart.md) |
| Carrying a project that already builds | [Moving a project across](migrate.md) |
| Looking for a flag or an opcode | [The command line](reference/cli.md), [Dialects](reference/dialects.md), [Instructions](reference/instructions.md) |

## What this book is

Generated wherever it can be. The instruction reference comes from the
instruction-set crate, the dialect table from the assembler itself, and the
figures on [Why asm198x](why.md) from the recorded verdicts they describe. The
build fails if any of them falls behind what it describes.

So a number on a page is a number something counted. Where we knowingly differ
from a reference assembler, [Where we differ](divergences.md) says so.
