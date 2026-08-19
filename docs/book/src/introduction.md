# asm198x

Assemblers and disassemblers for retro CPUs. One binary, no runtime
dependencies, the same interface on macOS, Linux and Windows.

The claim it is built around is narrow and testable: **real-world source for a
machine assembles unchanged**. Not "a 6502 assembler" — an assembler that reads
what ACME reads and emits what ACME emits, and separately one that reads what
ca65 reads. A front-end matches an existing assembler rather than inventing a
house syntax, because source people already have is the source that matters.

```sh
asm198x prog.asm -o prog.bin           # assemble
asm198x disasm prog.bin                # disassemble to stdout
asm198x fmt prog.asm                   # reformat, to stdout
```

## How that claim is checked

Byte-identical output is easy to assert and hard to keep true, so it is checked
against the real tools rather than against our reading of their manuals.

- **Differential suites** assemble the same source with `acme`, `ca65`, `pasmo`,
  `sjasmplus`, `lwasm`, `vasm`, `rgbasm` and `asl`, and compare bytes.
- **A verdict corpus** records what those tools actually produced, keyed on the
  tool's own version string. Every recorded fact is replayed in CI on machines
  with none of those assemblers installed, so a contributor can prove the claim
  in a pull request without installing anything.
- **Round trips** assemble, disassemble and reassemble, and must land on the
  same bytes.

Where we knowingly differ from a reference, the difference is recorded as a
tracked divergence rather than quietly tolerated — and the corpus fails the
build if one starts matching again, so the list cannot rot in either direction.

## What this book is

The parts that can be generated are generated, from the binary and from the
instruction-set crate, so they cannot fall behind the thing they describe. The
dialect table on [Dialects](dialects.md) is built by the assembler itself; the
build fails if it disagrees with what `--dialect` accepts.

The prose is reserved for what genuinely needs explaining, and tries not to
duplicate anything a machine could state more reliably.
