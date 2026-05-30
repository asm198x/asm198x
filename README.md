# Asm198x

A family of modern, single-binary assemblers (and, in time, disassemblers) for
the retro CPUs of the 198x era. One tool, consistent across machines, built to
still run in ten years.

## Why

The period assemblers are dead-OS binaries — you need DOSBox or a full emulator
just to invoke them. The modern community assemblers mostly work, but they are
fragmented: a different tool, syntax dialect, and build dance per machine, much
of it unmaintained C. Asm198x is one statically-linked, cross-platform,
well-documented toolchain that spans the family's CPUs. *Rescue beats replace.*

## Status

Early. The 6502 assembler does a real vertical slice: most of the documented
instruction set, the common addressing modes, labels, `.org`/`.byte`/`.word`,
and `<`/`>` low/high-byte operators. It is **not yet** source-compatible with
an existing 6502 dialect — that is the goal (see [`decisions/`](decisions/)).

```sh
cargo run -- examples/countdown.s -o countdown.bin
# assembled 11 byte(s) at $0200 -> countdown.bin
```

## Architecture

Two crates today, split only where a boundary is real:

| Crate | Role |
|-------|------|
| [`isa`](crates/isa) | Declarative instruction-set specs — the single source of truth for encoding (mnemonic ↔ opcode ↔ operand layout ↔ cycles ↔ flags). Zero dependencies. The neutral layer Emu198x will validate its decoders against. |
| [`asm198x`](crates/asm198x) | The assembler engine plus the 6502 dialect, as a library, and the `asm198x` CLI binary. |

The engine-vs-dialect split and per-CPU `isa` crates are deferred until a
second CPU makes those seams real. The `isa` crate stays standalone so Emu198x
can depend on it without pulling in the assembler.

This is the **assembler** pillar of the 198x family, a sibling to
[Code198x](../../Code198x) (curriculum) and [Emu198x](../../Emu198x) (emulator),
built on the same shared hardware reference. The binding architecture decision
lives in the umbrella record:
[`../../decisions/asm198x-and-shared-isa-spec.md`](../../decisions/asm198x-and-shared-isa-spec.md).

## Build

```sh
cargo build        # builds the CLI (default-members)
cargo test         # featherweight — no GUI/graphics deps
```

## Licence

GPL-2.0-or-later, matching the family.
