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

## Defined today (46)

`ADD` `ADDI` `ADDQ` `AND` `ANDI` `BEQ` `BGE` `BGT` `BLE` `BLT` `BMI` `BNE` `BPL`
`BRA` `BSET` `BSR` `BTST` `CLR` `CMP` `CMPI` `DBF` `DBRA` `DIVU` `EOR` `EORI`
`EXT` `LEA` `LSL` `LSR` `MOVE` `MOVEM` `MOVEQ` `MULU` `NEG` `NOP` `NOT` `OR`
`ORI` `RTS` `SEQ` `SNE` `SUB` `SUBI` `SUBQ` `SWAP` `TST`

## Burndown (priority order)

Highest first — control flow and common data movement make the assembler usable
for real programs; condition-code variants are mechanical breadth.

- [~] **1. Control flow** — **done:** `JMP`, `JSR` (control-addressing EA, reusing
      LEA's), `RTE`, `RTR`, `TRAPV`, `RESET`, `ILLEGAL` — byte-identical vs vasm
      (conformance sweep + `m68k_control_flow` decode test). **Remaining:** `TRAP`
      (4-bit vector packed in the opcode), `STOP` (`#imm16`), `CHK` (`<ea>,Dn`) —
      each needs a small new operand slot, so deferred to a follow-up increment.
- [ ] **2. Data movement** — `MOVEA`, `PEA`, `LINK`, `UNLK`, `EXG`, `MOVEP`,
      `MOVE` to/from `CCR`/`SR`/`USP`.
- [~] **3. Arithmetic / logic** — **done:** `MULS`, `DIVS` (mirror MULU/DIVU).
      **Remaining:** `ADDX`, `SUBX`, `NEGX`, `CMPM`, `ABCD`, `SBCD`, `NBCD`,
      `TAS` (`ADDX`/`SUBX`/`ABCD`/`SBCD`/`CMPM` need reg-reg/predec slot work).
- [x] **4. Bit ops** — `BCHG`, `BCLR` done (mirror BSET; `BSET`/`BTST` already
      present).
- [~] **5. Shifts / rotates** — **done:** `ASL`, `ASR`, `ROL`, `ROR`, `ROXL`,
      `ROXR` register forms (immediate- and register-count), mirroring LSL/LSR.
      **Remaining:** the memory-shift-by-1 forms (`$E0C0`…) — also absent for
      `LSL`/`LSR`, so a shared follow-up.
- [ ] **6. Condition-code breadth** — remaining `Scc` (have `SEQ`/`SNE`),
      remaining `Bcc` (`BHI`/`BLS`/`BCC`/`BCS`/`BVC`/`BVS`), the `DBcc` variants.
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
