# asm198x

Assemblers and disassemblers for retro CPUs. One binary, no runtime
dependencies, the same interface on macOS, Linux and Windows.

asm198x is built to assemble real-world source unchanged. Each front-end
matches an existing assembler: the ACME front-end takes the source ACME takes
and emits the bytes ACME emits, and the ca65 front-end does the same for ca65.

```sh
asm198x prog.asm -o prog.bin           # assemble
asm198x disasm prog.bin                # disassemble to stdout
asm198x fmt prog.asm                   # reformat, to stdout
```

## How that is checked

Output is compared against the tools themselves.

- **Differential suites** assemble the same source with `acme`, `ca65`, `pasmo`,
  `sjasmplus`, `lwasm`, `vasm`, `rgbasm` and `asl`, and compare bytes.
- **A verdict corpus** records what those tools produced, keyed on the tool's
  own version string. CI replays every recorded fact on machines with none of
  those assemblers installed, so you can check a pull request without
  installing any of them.
- **Round trips** assemble, disassemble and reassemble, and must land on the
  same bytes.

Known differences from a reference tool are recorded as tracked divergences,
and the corpus fails the build if one starts matching again.

## What this book is

Generated wherever it can be: the instruction reference comes from the
instruction-set crate, and the dialect table from the assembler itself. The
build fails if either falls behind what it describes.
