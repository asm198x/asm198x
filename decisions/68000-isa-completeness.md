# 68000 ISA completeness — burndown

**Status:** ✅ Complete — base-68000 ISA fully modelled (2026-06-04). Kept as a
record of the gap and how it was closed.

**Date:** 2026-06-04.

## What this is

`isa::m68k` (`crates/isa/src/m68k.rs`) — the shared 68000 spec consumed by **both**
the assembler (vasm dialect, `m68k::SET.instruction()`) and the disassembler
(`isa-disasm`, `decode_m68k` iterates `m68k::SET`) — began as a **curriculum
subset**: 46 mnemonics, roughly half the base-68000 ISA. The spec was authored to
match the Amiga curriculum and validated byte-identical against vasm on that
corpus, which is why the roadmap read "✅ done." The rung-1 cross-check against
Emu198x's independent decoder made the gap measurable — see the umbrella
[`rung1-wiring.md`](../../../decisions/rung1-wiring.md) (2026-06-04 entries).

All seven families below are now landed: the spec covers the full base-68000
instruction set, every addition validated byte-identical against vasm
(assemble + disassemble) and exercised by the conformance sweep (~41k decodable
instructions). This record tracked filling that gap.

## Why it matters — it's the *shared* spec

Because `m68k::SET` feeds both tools, every missing mnemonic fails twice:

- **assemble** — `assemble_vasm` rejects it with "unknown instruction" (verified
  for `jsr`, `jmp`, `movea`, `muls`, `asl`); real 68000 code that uses it won't
  assemble.
- **disassemble** — `disassemble_68000` renders it `dc.w`; the Emu198x 68000
  cross-check can't see Emu decode bugs there (it scopes to the shared surface).

Filling a family lights up assemble + disassemble + cross-check coverage at once.

## Defined today

The authoritative list is `m68k::SET` in `crates/isa/src/m68k.rs` — don't mirror
it here (it drifts). The starting point was a **46-mnemonic curriculum subset**
(`ADD` `ADDI` `ADDQ` `AND` `ANDI` `Bcc`×8 `BSET` `BSR` `BTST` `CLR` `CMP` `CMPI`
`DBF`/`DBRA` `DIVU` `EOR` `EORI` `EXT` `LEA` `LSL` `LSR` `MOVE` `MOVEM` `MOVEQ`
`MULU` `NEG` `NOP` `NOT` `OR` `ORI` `RTS` `Scc`×2 `SUB` `SUBI` `SUBQ` `SWAP`
`TST`). The burndown below tracks what's been added since; families 3–6 are
complete, families 1–2 and 7 have remaining work flagged inline.

## Burndown (priority order)

Highest first — control flow and common data movement make the assembler usable
for real programs; condition-code variants are mechanical breadth.

- [x] **1. Control flow** — done: `JMP`, `JSR`, `RTE`, `RTR`, `TRAPV`, `RESET`,
      `ILLEGAL`, `CHK`, `STOP` (`#imm16` reuses `ImmWord`), and `TRAP` (4-bit
      vector via the new `Slot::Vec4`, packed in the opcode's low nibble).
- [x] **2. Data movement** — done: `PEA` (control EA), `UNLK` (`An`), `LINK`
      (`An` + `ImmWord` displacement), `MOVEA` (An-destination MOVE, listed
      before MOVE so it wins the decode; no new slot), `EXG` (three register-pair
      kinds plus reversed source order; reuses `Dn`/`An`), the control-register
      moves `MOVE <ea>,CCR` / `MOVE <ea>,SR` / `MOVE SR,<ea>` / `MOVE USP,An` /
      `MOVE An,USP` (new `Slot::Ccr`/`Sr`/`Usp` + `ccr`/`sr`/`usp` parser tokens;
      `MOVE CCR,<ea>` is 68010+ and intentionally absent), and `MOVEP` (new
      `Slot::MovepDisp` carrying `d16(Ay)` — Ay in bits 0–2 plus a mandatory
      displacement word; both directions and sizes).
- [x] **3. Arithmetic / logic** — done: `MULS`, `DIVS` (mirror MULU/DIVU),
      `NEGX`, `NBCD`, `TAS` (slot-reusing single-EA), and `ADDX`/`SUBX`/`CMPM`/
      `ABCD`/`SBCD` via a new `Slot::AddrIndirect { shift, mode }` (the register
      number sits in the opcode, no 6-bit EA field) — both `Dn,Dn` and
      `-(An),-(An)` / `(An)+,(An)+` shapes, byte-identical vs vasm.
- [x] **4. Bit ops** — `BCHG`, `BCLR` done (mirror BSET; `BSET`/`BTST` already
      present).
- [x] **5. Shifts / rotates** — register forms (immediate/register count) and the
      memory-shift-by-1 forms (`$E0C0`…), for all eight (`ASL`/`ASR`/`LSL`/`LSR`/
      `ROL`/`ROR`/`ROXL`/`ROXR`). Byte-identical vs vasm.
- [x] **6. Condition-code breadth** — done: the 6 remaining `Bcc`
      (`BHI`/`BLS`/`BCC`/`BCS`/`BVC`/`BVS`), all 14 remaining `Scc`, and all 15
      `DBcc` variants (mirror BEQ/SEQ/DBF; cc in bits 8–11). Byte-identical vs
      vasm (Scc via the sweep; Bcc/DBcc — position-dependent — via a direct
      assembler check), plus an `m68k_condition_codes` decode test.
- [x] **7. Immediate to CCR/SR** — done: `ANDI`/`ORI`/`EORI #imm,CCR/SR`
      (`$x03C`/`$x07C`) via `Slot::Ccr`/`Sr` and a single `ImmWord` (byte in the
      word's low half for CCR). These occupy the immediate-EA bit pattern, which
      is illegal as a normal alterable EA, so they never shadow the generic
      `#imm,<ea>` forms. Byte-identical vs vasm.

## How to land a family

1. Add the `Insn` + `Form`(s) to `isa::m68k::SET`, encodings cited to the primary
   reference (Motorola M68000 PRM), like the existing entries.
2. **Table-only vs encoding work.** Families reusing existing `Slot`s (most
   shifts/rotates, bit ops, the arithmetic-with-EA ops) are table-only — the vasm
   dialect and `decode_m68k` pick them up for free. Others need new
   `Slot`/`match_form` support: `MOVEP` (special mode), `LINK`/`UNLK` (`An` +
   displacement), `EXG` (two registers), CCR/SR targets (a dedicated slot),
   `JMP`/`JSR` (the control-addressing EA subset).
3. **Validate.** The conformance sweep (`spec_sweep_matches_reference`) covers a
   newly-decodable family against vasm automatically; add curriculum/round-trip
   cases as warranted. The Emu198x 68000 cross-check's shared surface grows for
   free as the skip count falls.

## Definition of done — met

The base-68000 ISA assembles and disassembles byte-identically against vasm, and
the Emu198x 68000 cross-check's skip count drops to only genuinely
ambiguous/illegal encodings (not "unimplemented"). All seven families are landed,
so the roadmap's 68000 row returns to a plain ✅.

Absent from this base-68000 spec (by design): the 68010+ additions
(`MOVE CCR,<ea>`, `MOVEC`, `MOVES`, `RTD`, `BKPT`) and the 68020+ extensions.

**Deferred, not ruled out — the 68020 is anticipated.** The A1200 (68020, AGA) is
in family scope, so a 68020 target will eventually be needed; Emu198x already
carries AGA scaffolding. We are holding off until an A1200-class dev/emulation
need is real. When it comes, the cost is uneven: the new *instructions*
(bit-field ops, 32×32 `MULS.L`/`DIVxL.L`, `CAS`/`CAS2`, `PACK`/`UNPK`,
`TRAPcc`, `EXTB.L`, `Bcc.L`, …) are table-and-slot work like this burndown, but
the new *addressing modes* (memory indirect, scaled index, the full multi-word
extension format) are a substantial EA-decoder change — larger than all the
base-ISA gaps combined, and the part to scope carefully. The 68010 step, by
contrast, is small and instruction-only. See
[`packaging-and-cpu-roadmap.md`](packaging-and-cpu-roadmap.md) § CPU roadmap.

A known minor gap in the base spec: an out-of-byte-range immediate to CCR
(`andi #$1234,ccr`) is not rejected by our assembler the way vasm rejects it —
the encoding still places the low byte. The conformance sweep never hits this
(its synthesized immediates are byte-range), and it only affects malformed
hand-written source.

## Provenance

Surfaced by the rung-1 cross-check (umbrella `rung1-wiring.md`, 2026-06-03/04) and
verified directly: `assemble_vasm` rejects `jsr`/`jmp`/`movea`/`muls`/`asl` as
"unknown instruction", and `disassemble_68000` renders ~16k opcodes as `dc.w`.
