# Asm198x

[![CI](https://github.com/asm198x/asm198x/actions/workflows/ci.yml/badge.svg)](https://github.com/asm198x/asm198x/actions/workflows/ci.yml)

A family of modern, single-binary assemblers and disassemblers for the retro
CPUs of the 198x era. One tool, source-compatible with the dialects people
already use, built to still run in ten years.

## Why

The period assemblers are dead-OS binaries — you need DOSBox or a full emulator
just to invoke them. The modern community assemblers mostly work, but they are
fragmented: a different tool, syntax dialect, and build dance per machine, much
of it unmaintained C. Asm198x is one statically-linked, cross-platform,
well-documented toolchain that spans the family's CPUs. *Rescue beats replace.*

The guiding rule is **source-compatibility**: real-world source for a machine
should assemble unchanged. Rather than invent a house syntax, each front-end
matches an existing dialect, and the output is validated byte-for-byte against
that tool on real curriculum code.

## What works

Two CPUs, four source dialects, both directions, all validated byte-identical
against the reference tool on the [Code198x](../../Code198x) curriculum:

| CPU | Dialects | Target | Disassembler |
|-----|----------|--------|--------------|
| 6502 | **acme** (C64), **ca65** (NES) | flat binary / `.nes` ROM | ✅ |
| Z80  | **pasmo**/**pasmonext**, **sjasmplus** | flat binary, incl. Z80N | ✅ |

- **acme** — `*=`, `!byte`/`!word`/`!fill`/`!text`/`!scr`, `name = value`,
  anonymous `-`/`+` labels, conditional assembly (`!if`/`!ifdef`/`!ifndef`),
  hex-width-aware addressing. The whole C64 curriculum (80 units) assembles
  byte-identical to `acme -f cbm`.
- **ca65** — `.segment`/`.byte`/`.word`/`.res`, `@cheap` locals, plus a
  **bounded ld65-style linker** for the standard NROM config, so it emits a
  finished `.nes` ROM (iNES header + PRG + CHR). All 32 NES units match
  `ca65 + ld65`.
- **Z80** — the complete documented instruction set (base, `ED`, `CB`, `DD`/`FD`
  index registers) plus the Spectrum Next's Z80N, `$`-as-PC, and sjasmplus-style
  local labels. The Gloaming Spectrum curriculum (20 units) matches both
  `pasmonext` and `sjasmplus`.
- **Disassembly** is driven by the same spec the assemblers emit from, so
  assemble → disassemble → reassemble round-trips byte-for-byte.

```sh
asm198x --dialect acme   examples/countdown.s -o countdown.bin   # C64 6502
asm198x --dialect ca65   game.asm             -o game.nes        # NES (assemble + link)
asm198x --dialect pasmo  main.asm             -o main.bin        # ZX Spectrum Z80
asm198x --disasm --dialect 6502 --org 0x0200 countdown.bin       # back to source
```

## Architecture

Two crates, split only where a boundary is real:

| Crate | Role |
|-------|------|
| [`isa`](crates/isa) | Declarative instruction-set specs — the single source of truth for encoding (mnemonic ↔ opcode ↔ operand layout ↔ cycles ↔ flags), for `mos6502` and `z80` (with the Z80N extension set). Zero dependencies; the neutral layer Emu198x will validate its decoders against. |
| [`asm198x`](crates/asm198x) | The library — a dialect-agnostic engine, the shared per-CPU cores (`mos6502`, `z80`), the dialect front-ends, the bounded NES linker, and the disassembler — plus the `asm198x` CLI. |

The **engine ↔ dialect ↔ spec** seam is what lets one binary span many CPUs and
dialects: a dialect is a front-end module, a CPU is a spec. Most dialects produce
a flat `Assembly` through the engine; **ca65** is the exception — it assembles
and links to a ROM, bypassing the flat engine while reusing the shared 6502 core.
See the top of [`crates/asm198x/src/lib.rs`](crates/asm198x/src/lib.rs) and
[`decisions/syntax-stance.md`](decisions/syntax-stance.md).

This is the **assembler** pillar of the 198x family, a sibling to
[Code198x](../../Code198x) (curriculum) and [Emu198x](../../Emu198x) (emulator),
built on the same shared hardware reference. The binding architecture decision
lives in the umbrella record:
[`../../decisions/asm198x-and-shared-isa-spec.md`](../../decisions/asm198x-and-shared-isa-spec.md).

## Build and test

```sh
cargo build        # builds the CLI (default-members)
cargo test         # unit tests — featherweight, no GUI/graphics deps

# Byte-identity against the real toolchains and the Code198x corpus
# (needs acme, ca65/ld65, pasmo, sjasmplus on PATH and the sibling checkout):
cargo test --test curriculum -- --ignored --nocapture
```

## Licence

GPL-2.0-or-later, matching the family.
