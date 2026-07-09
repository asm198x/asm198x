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
should assemble unchanged. Instead of inventing a house syntax, each front-end
matches an existing dialect where compatibility matters, and output is validated
byte-for-byte against reference tools and fixtures.

## What works

Asm198x now covers a broad 8-bit and 16-bit CPU surface through source-compatible dialect front-ends, shared ISA specs, and spec-driven disassemblers. Current families include 6502/65816/HuC6280, Z80/Z80N/SM83/8080/Z8000, 6800/6809/68000, CDP1802, MCS-48/8048, SC/MP, Fairchild F8, Signetics 2650, TMS7000, PDP-11, TMS9900, and CP1610.

Representative validated front doors:

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
- **Additional CPU families** are validated through spec sweeps, opcode-space sweeps, targeted round trips, and reference-assembler differentials. Keep detailed CPU coverage in tests, decisions, and `CLAUDE.md`, not duplicated here.
- **Disassembly** is driven by the same specs the assemblers emit from, so assemble → disassemble → reassemble round-trips stay byte-for-byte where that surface is implemented.

```sh
asm198x --dialect acme       examples/countdown.s -o countdown.bin   # C64 6502
asm198x --dialect ca65       game.asm             -o game.nes        # NES (assemble + link)
asm198x --dialect pasmo      main.asm             -o main.bin        # ZX Spectrum Z80
asm198x --dialect vasm --exe game.s               -o game            # Amiga 68000 (hunk exe)
asm198x --dialect lwasm      game.s               -o game.bin        # Dragon/CoCo 6809
asm198x --disasm --dialect 6502 --org 0x0200      countdown.bin      # back to source
```

## Architecture

Four crates, split only where a boundary is real:

| Crate | Role |
|-------|------|
| [`isa`](crates/isa) | Dependency-free declarative instruction-set specs: the single source of truth for encoding. |
| [`isa-disasm`](crates/isa-disasm) | Spec-driven disassemblers that decode against `isa` without pulling in the assembler. |
| [`debug198x`](crates/debug198x) | Cross-CPU debug-info sidecar format for line/address maps, typed symbols, and sections. |
| [`asm198x`](crates/asm198x) | Assembler library, dialect front-ends, shared engine, formatter, diagnostics contract, bounded linked-output paths, and the `asm198x` CLI. |

The **engine ↔ dialect ↔ spec** seam is what lets one binary span many CPUs and
dialects: a dialect is a front-end module, a CPU is a spec. Most dialects produce
a flat `Assembly` through the engine; **ca65** (→ `.nes` ROM) and **vasm**
(→ Amiga hunk executable) are the exceptions — they assemble *and link* to a
finished image, bypassing the flat engine while reusing the shared core.
See the top of [`crates/asm198x/src/lib.rs`](crates/asm198x/src/lib.rs) and
[`decisions/syntax-stance.md`](decisions/syntax-stance.md).

Asm198x owns the assembler/disassembler and executable ISA-spec layer for the 198x family, built on the same shared hardware reference as the curriculum, emulator, build-tools, catalogue, and future workbench projects. The binding architecture decision lives in the umbrella record:
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
