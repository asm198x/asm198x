# 68000 ISA completeness — burndown

**Status:** Active tracking. Asm198x.

**Date:** 2026-06-04.

## What this is

`isa::m68k` (`crates/isa/src/m68k.rs`) — the shared 68000 spec consumed by **both**
the assembler (vasm dialect, `m68k::SET.instruction()`) and the disassembler
(`isa-disasm`, `decode_m68k` iterates `m68k::SET`) — is a **curriculum subset**:
46 mnemonics, roughly half the base-68000 ISA. The spec was authored to match the
Amiga curriculum and validated byte-identical against vasm on that corpus, which
is why the roadmap read "✅ done." The rung-1 cross-check against Emu198x's
independent decoder made the gap measurable — see the umbrella
[`rung1-wiring.md`](../../../decisions/rung1-wiring.md) (2026-06-04 entries).

This record tracks filling the spec out to the full base-68000 ISA.

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

- [~] **1. Control flow** — **done:** `JMP`, `JSR`, `RTE`, `RTR`, `TRAPV`,
      `RESET`, `ILLEGAL`, `CHK`, and `STOP` (`#imm16` reuses `ImmWord`).
      **Remaining:** `TRAP` (4-bit vector packed in the opcode — needs a new slot).
- [~] **2. Data movement** — **done:** `PEA` (control EA), `UNLK` (`An`), `LINK`
      (`An` + `ImmWord` displacement). **Remaining:** `MOVEA`, `EXG`, `MOVEP`,
      `MOVE` to/from `CCR`/`SR`/`USP` — all need new slots.
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
- [ ] **7. Immediate to CCR/SR** — `ANDI`/`ORI`/`EORI #imm,CCR/SR`
      (`$003C`/`$007C`, …) — the forms the rung-1 ORI/ANDI/EORI work explicitly
      left unmodelled (need a dedicated CCR/SR operand slot).

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

## Definition of done

The base-68000 ISA assembles and disassembles byte-identically against vasm, and
the Emu198x 68000 cross-check's skip count drops to only genuinely
ambiguous/illegal encodings (not "unimplemented"). At that point the roadmap's
68000 row returns to a plain ✅.

## Provenance

Surfaced by the rung-1 cross-check (umbrella `rung1-wiring.md`, 2026-06-03/04) and
verified directly: `assemble_vasm` rejects `jsr`/`jmp`/`movea`/`muls`/`asl` as
"unknown instruction", and `disassemble_68000` renders ~16k opcodes as `dc.w`.
