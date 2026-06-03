# Asm198x

A family of modern, single-binary assemblers and disassemblers for the 198x
family's target CPUs. One of three sibling projects under the `198x/` umbrella;
see [`../../CLAUDE.md`](../../CLAUDE.md) for umbrella context and cross-project rules,
and [`../../decisions/sibling-project-coordination.md`](../../decisions/sibling-project-coordination.md)
for the sibling relationship (Asm198x is the third sibling, peer to Code198x
and Emu198x — not a child of either).

## What this is

Modern, statically-linked, cross-platform assemblers to replace toolchains that
are getting hard to run: period assemblers need dead-OS emulation; modern
community ones are fragmented (a different tool and dialect per machine). One
Rust workspace, many crates — *not* one repo per CPU. Boring technology;
rescue over replace.

## Binding architecture

Read [`../../decisions/asm198x-and-shared-isa-spec.md`](../../decisions/asm198x-and-shared-isa-spec.md)
before changing the crate structure or the ISA layer. The load-bearing points:

- **Shared declarative ISA spec.** The [`isa`](crates/isa) crate is the single
  source of truth for instruction *encoding*. Asm198x consumes it to assemble
  and disassemble; Emu198x validates its hand-written decoders against it. The
  spec is **authored** from the primary reference library (datasheets), **not
  extracted** from any emulator's decode loop.
- **`isa` stays dependency-free and standalone**, so Emu198x can depend on it
  without pulling in the assembler. It lives here for now; promotion to a
  neutral location is deferred until Emu198x actually consumes it.
- **Source-compatible per machine.** The dialect lives in each CPU's parser
  front-end; the encoding underneath is the shared spec. Detail and per-CPU
  dialect targets are in [`decisions/`](decisions/).

## Crate layout

Two crates today; split further only when the per-CPU `isa` boundary or
Emu198x's consumption makes it real.

- [`crates/isa`](crates/isa) — instruction-set specs (types + `mos6502` + `z80`
  + `m68k`; the Z80 set includes the Z80N extensions). Zero dependencies.
- [`crates/isa-disasm`](crates/isa-disasm) — the spec-driven disassemblers
  (6502, Z80, 68000, 6809, 65816), decoding against `isa`. Depends only on `isa` + std, so
  Emu198x can consume disassembly without the assembler. See
  [`decisions/disassembler-crate.md`](decisions/disassembler-crate.md).
- [`crates/asm198x`](crates/asm198x) — the library (dialect-agnostic engine,
  the shared per-CPU cores, the dialect front-ends) and the `asm198x` CLI. It
  re-exports the disassembler from `isa-disasm`.

Delivered so far, all validated byte-identical against the real tool on the
curriculum corpus:

- **6502** — `acme` (C64) and `ca65` (NES) front-ends over a shared
  `dialects::mos6502` core, plus a spec-driven 6502 disassembler. ca65 also
  carries a **bounded ld65-style linker** for the fixed NES config (it emits a
  `.nes` ROM, not a flat binary — see the flat-vs-linked note in the library
  crate docs and `decisions/syntax-stance.md`).
- **Z80** — `pasmo`/`pasmonext` and `sjasmplus` front-ends over a shared
  `dialects::z80` core, the Z80N target, and a spec-driven Z80 disassembler.
- **6809** — `lwasm` front-end (`dialects::lwasm`) over the `isa::mos6809` spec,
  plus a spec-driven 6809 disassembler. First user of the engine's
  **computed-operand seam** (`Operation::Encoded` / `Piece`), for CPUs whose
  operands are computed rather than fixed-width slots. All addressing modes
  (including the full indexed set — the computed postbyte + 0/1/2 extension
  bytes, auto inc/dec, accumulator offsets, indirect, PC-relative), the register
  ops (`tfr`/`exg`/`pshs`/`puls`/`pshu`/`pulu`), and `org`/`equ`/`fcb`/`fdb`/
  `fcc`/`rmb` are landed and validated byte-identical against `lwasm --6809
  --raw`, with assemble→disassemble→reassemble round-trip.
- **65816** — `ca65` syntax (`dialects::ca65_816`) as a **target extension** of
  the 6502: `isa::mos6502` (primary) + `isa::mos65816` (extension), the
  `z80::NEXT` mechanism. Native-mode core: the `m`/`x` immediate width
  (`.a8`/`.a16`/`.i8`/`.i16` → `"immediate"`/`"immediate16"` fixed-slot forms,
  no `Encoded` seam), all new addressing modes (long, `[dp]`, stack-relative, …),
  `z:`/`a:`/`f:` size forces with fall-up, long calls/jumps, the new
  instructions, `mvn`/`mvp`, `cop`/`wdm`, and the `^` bank-byte operator. The
  engine carries 24-bit operands and an `i64` symbol table. A spec-driven
  disassembler tracks `m`/`x` via `rep`/`sep` (emitting `.aXX`/`.iXX`) so
  width-switching code round-trips. Validated byte-identical against `ca65 --cpu
  65816` (flat). Deferred: `.smart` and `@cheap` locals (source conveniences).

The engine ↔ dialect ↔ spec seam (and, for ca65, the assemble + link path that
bypasses the flat engine) is documented at the top of `crates/asm198x/src/lib.rs`.
The encoding-model taxonomy (fixed slots / field-packed / computed operand) and
the computed-operand seam are in `../../decisions/packaging-and-cpu-roadmap.md`.

## How correctness is checked

Four layers, each against the real reference assemblers (all `#[ignore]`d — they
need the tools installed — and degrading gracefully when one is absent):

- **`tests/curriculum`** — curated curriculum programs, byte-identical to the
  reference tool, plus assemble→disassemble→reassemble round-trip (our own asm).
- **`tests/conformance`** — three checks, all making the reference tool the
  arbiter by reusing the disassemblers (synthesise bytes → disassemble →
  reassemble with the *reference*): every form-based spec's opcode
  (`spec_opcodes_match_reference`: 6502/Z80/65816), an opcode-space sweep for
  the non-form specs (`spec_sweep_matches_reference`: 6809 and 68000 — ~33k
  decodable encodings; 68000 PC-relative EA is the one skipped case, see the
  decision), and a seeded differential fuzzer over random programs reassembled
  by both our asm and the reference (`differential_fuzz`).

See [`decisions/spec-conformance-and-fuzzing.md`](decisions/spec-conformance-and-fuzzing.md).

## Build-time discipline

The workspace bakes in the levers that keep builds fast — `default-members`
scoped to the CLI, and a `[profile.dev]` that drops full debuginfo (the biggest
`cargo test` cost). Assemblers are featherweight (no `wgpu`/audio/GUI), so this
should stay in the seconds. If a build ever feels slow, the cause is the
dependency graph or profile — never the repo boundary. (Background: this was
measured on Emu198x, whose pain was `cargo test` linking hundreds of
debuginfo-heavy binaries, not its crate count.)

## Where things live

- [`decisions/`](decisions/) — Asm198x-only decisions (syntax stance, dialect
  targets). Cross-project decisions live in [`../../decisions/`](../../decisions/).
- [`crates/`](crates/) — the Rust workspace.
- [`examples/`](examples/) — sample source.

Hardware facts come from the umbrella primary library at [`../../reference/`](../../reference/)
and syntheses at [`../../syntheses/`](../../syntheses/), per
[`../../decisions/shared-hardware-reference-canon.md`](../../decisions/shared-hardware-reference-canon.md).
The `isa` spec is the machine-readable distillation of the encoding slice of
those facts; it cites the library, not the other way round.
