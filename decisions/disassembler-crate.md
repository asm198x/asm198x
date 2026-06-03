# Decision: the disassembler is its own dependency-free crate

**Status:** Active. Binding for Asm198x.

**Date:** 2026-06-03.

## The decision

The spec-driven disassemblers (6502, Z80, 68000, 6809, 65816) live in a separate crate,
[`crates/isa-disasm`](../crates/isa-disasm), depending only on `isa` + std —
the same neutral footing as `isa` itself. `asm198x` depends on `isa-disasm` and
**re-exports** it (`disassemble_6502`/`disassemble_z80`/`disassemble_68000`/
`disassemble_6809`/`disassemble_65816`, their `listing_*` forms, and `Line`), so the `asm198x`
library API and CLI are unchanged.

This makes disassembly consumable **without the assembler** — no parser, no
engine, no CLI, no dialect front-ends. The motivating consumer is **Emu198x**,
whose MCP/debug tooling wants to render running code (the Atari 800XL 6502
tool today; the Amiga 68000 next) but must not pull in the whole toolchain.

## Why now

The disassembler had matured across all three CPUs (6502, Z80, and now the
68000), so its shape — and its dependency surface (`isa` + std, nothing else) —
was settled enough to lift out cleanly. It already imported only `isa`; the
extraction was a move plus a re-export, not a redesign.

## What moved, what stayed

- **Moved to `isa-disasm`:** all of `disasm.rs` (decode + render for the three
  CPUs) and the **decode-only** unit tests (single-instruction decode and
  rendering), which need no assembler.
- **Stayed in `asm198x`:** the **round-trip** tests (assemble → disassemble →
  reassemble), in `src/roundtrip_tests.rs`. They need both halves, and the
  68000 curriculum round-trip reaches `dialects::vasm::assemble_with(.., false)`
  (a crate-internal `-no-opt` path), so they must be unit tests inside this
  crate, not external integration tests.

## The `isa` → `isa-core` + `isa-*` split: deferred

`isa-disasm` depends on the whole `isa` crate (all CPUs). Splitting `isa` into a
shared `isa-core` (types) plus per-CPU `isa-mos6502`/`isa-z80`/`isa-m68k` crates
is **not done** and not yet warranted: `isa` is small, zero-dependency, and a
consumer that only wants 6502 disassembly still compiles the whole thing in
negligible time. Revisit the split only if (a) a CPU's spec grows large enough
that compiling unused ones is a real cost, or (b) Emu198x wants to depend on a
single CPU's spec in isolation. Until then, one `isa` crate keeps the
cross-CPU types in one place and avoids premature crate sprawl (YAGNI).

## How Emu198x consumes it

Emu198x adds a path dependency to its own `Cargo.toml`:

```toml
[dependencies]
isa-disasm = { path = "../Asm198x/asm198x/crates/isa-disasm" }
```

(adjusting the relative path to wherever Emu198x sits beside `Asm198x/`). It
then calls `isa_disasm::disassemble_6502(code, origin)` /
`disassemble_68000(code, origin)` and renders `Line { addr, bytes, text }`.
Nothing else from the toolchain is pulled in. This mirrors the existing plan
for `isa` (the neutral spec layer Emu198x validates its hand-written decoders
against) — `isa-disasm` is the second neutral crate on that seam. See the
umbrella `asm198x-and-shared-isa-spec.md`.

## Drift triggers

- **"Just put the disassembler back in the `asm198x` library."** No — that
  re-couples it to the assembler and forces Emu198x to depend on the parser,
  engine, and CLI. Keep it on `isa` + std only.
- **"Add a dependency to `isa-disasm` for convenience."** No — its whole value
  is the minimal surface. Anything heavier belongs in `asm198x`.
- **"Split `isa` per CPU now."** Not until a concrete cost appears (see above);
  one zero-dep `isa` crate is the current call.
