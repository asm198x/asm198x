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

**Five CPUs, both directions** — every front-end validated byte-identical
against the reference tool (on the [Code198x](../../Code198x) curriculum where one
exists, against the reference assembler directly otherwise):

| CPU | Dialect(s) | Output | Disassembler |
|-----|-----------|--------|--------------|
| 6502 | **acme** (C64), **ca65** (NES) | flat binary / `.nes` ROM | ✅ |
| Z80 | **pasmo**/**pasmonext**, **sjasmplus** | flat binary, incl. Z80N | ✅ |
| 68000 | **vasm** (Motorola syntax) | flat binary / Amiga hunk executable | ✅ |
| 6809 | **lwasm** | flat binary | ✅ |
| 65816 | **ca65** (`--cpu 65816`) | flat binary | ✅ |

- **6502 / acme** — `*=`, `!byte`/`!word`/`!fill`/`!text`/`!scr`, `name = value`,
  anonymous `-`/`+` labels, conditional assembly (`!if`/`!ifdef`/`!ifndef`),
  hex-width-aware addressing. The whole C64 curriculum (80 units) assembles
  byte-identical to `acme -f cbm`.
- **6502 / ca65** — `.segment`/`.byte`/`.word`/`.res`, `@cheap` locals, plus a
  **bounded ld65-style linker** for the standard NROM config, so it emits a
  finished `.nes` ROM (iNES header + PRG + CHR). All 32 NES units match
  `ca65 + ld65`.
- **Z80** — the complete documented instruction set (base, `ED`, `CB`, `DD`/`FD`
  index registers) plus the Spectrum Next's Z80N, `$`-as-PC, and sjasmplus-style
  local labels. The Gloaming Spectrum curriculum (20 units) matches both
  `pasmonext` and `sjasmplus`.
- **68000 / vasm** — Motorola syntax over a field-packed core, emitting either a
  flat big-endian image or, with `--exe`, a loadable **Amiga hunk executable**
  (`-Fhunkexe -kick1hunks`: header, code/data/bss hunks, reloc32 tables). 32/32
  hunk-exe parity against `vasmm68k_mot`.
- **6809 / lwasm** — the full instruction set including the computed indexed
  addressing modes (postbyte + extension bytes, auto inc/dec, accumulator
  offsets, indirect, PC-relative) and the register ops (`tfr`/`exg`/`pshs`/…).
  Validated against `lwasm --6809 --raw`.
- **65816 / ca65** — native 16-bit mode as a target extension of the 6502:
  `m`/`x` width tracking (`.a8`/`.a16`/`.i8`/`.i16`), long / `[dp]` /
  stack-relative modes, `mvn`/`mvp`, `cop`/`wdm`, the `^` bank-byte operator,
  24-bit operands. Validated against `ca65 --cpu 65816`.
- **Disassembly** is driven by the same spec the assemblers emit from, so
  assemble → disassemble → reassemble round-trips byte-for-byte (the 65816
  disassembler even tracks `m`/`x` via `rep`/`sep`).

```sh
asm198x --dialect acme       examples/countdown.s -o countdown.bin   # C64 6502
asm198x --dialect ca65       game.asm             -o game.nes        # NES (assemble + link)
asm198x --dialect pasmo      main.asm             -o main.bin        # ZX Spectrum Z80
asm198x --dialect vasm --exe game.s               -o game            # Amiga 68000 (hunk exe)
asm198x --dialect lwasm      game.s               -o game.bin        # Dragon/CoCo 6809
asm198x --disasm --dialect 6502 --org 0x0200      countdown.bin      # back to source
```

## Architecture

Three crates, split only where a boundary is real:

| Crate | Role |
|-------|------|
| [`isa`](crates/isa) | Declarative instruction-set specs — the single source of truth for encoding (mnemonic ↔ opcode ↔ operand layout ↔ cycles ↔ flags) for `mos6502`, `z80` (with the Z80N extension set), `m68k`, `mos6809`, and `mos65816`. Zero dependencies; the neutral layer Emu198x will validate its decoders against. |
| [`isa-disasm`](crates/isa-disasm) | The spec-driven disassemblers (6502, Z80, 68000, 6809, 65816), decoding against the same `isa` data the assemblers emit from. Depends only on `isa` + std, so Emu198x can consume disassembly without pulling in the assembler. |
| [`asm198x`](crates/asm198x) | The library — a dialect-agnostic engine, the shared per-CPU cores, the dialect front-ends, and the bounded NES linker — plus the `asm198x` CLI. Re-exports the disassembler from `isa-disasm`. |

The **engine ↔ dialect ↔ spec** seam is what lets one binary span many CPUs and
dialects: a dialect is a front-end module, a CPU is a spec. Most dialects produce
a flat `Assembly` through the engine; **ca65** (→ `.nes` ROM) and **vasm**
(→ Amiga hunk executable) are the exceptions — they assemble *and link* to a
finished image, bypassing the flat engine while reusing the shared core.
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
