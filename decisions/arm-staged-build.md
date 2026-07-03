# ARM (ARM2 / ARMv2) — a staged build (scoped, not yet started)

**Status:** 📋 **Planned (scoped 2026-07-03).** The first 32-bit RISC and the
highest-value 32-bit door (Archimedes + later, via extension, GBA/DS). Scoping is
done, the arbiter is built and installed, the engine widening is understood.
Build not yet started.

## Scope: ARM2 (ARMv2, 26-bit) — the Archimedes

The first ARM target is the **ARM2 / VL86C010, 26-bit** part — exactly what the
umbrella primary sources document (`reference/by-topic/cpu-arm/cpu-arm-reference.md`
+ the Docling-extracted VTI ARM Databook 1990). Its defining traits:

- **32-bit RISC, load/store**, fixed 32-bit word-aligned instructions.
- **26-bit address space** (64 MB); the **PC and PSR are packed into R15** (PC in
  bits 25:2 as a word address, N/Z/C/V + I/F + 2 mode bits in the top/bottom).
- **Every instruction is conditional** — a 4-bit condition field in bits 31:28.
- The **barrel shifter** on the second data-processing operand.

**Out of scope for the first build** (later architectures / a target-extension,
the Z8001-over-Z8000 pattern): **Thumb**, the separate 32-bit CPSR/SPSR model,
halfword / signed loads, long multiply (`UMULL`/`SMULL`), and `SWP` — all
ARM3+/ARMv4T (the ARM7TDMI in the GBA/DS). Folding Thumb in now would mean a
whole second 16-bit instruction set with weaker primary sources; defer it.

## Arbiter: vasm ARM (`vasmarm_std`)

Unlike every CPU so far, **`asl` does not support ARM**, and no ARM assembler was
installed — the same "arbiter gate" as the blocked Wave-B CPUs, but resolvable:
built the **vasm ARM target** from source (`make CPU=arm SYNTAX=std`) and
installed `vasmarm_std` to `/opt/homebrew/bin`. Chosen over GNU `arm-none-eabi-as`
because it shares the author, syntax family, and `-Fbin` flat output of the
`vasmm68k_mot` we already use for the 68000, so it drops straight into the
existing differential / sweep harness. Invocation:

```
vasmarm_std -Fbin -m2 -o out.bin in.s
```

`-m2` selects ARM2; output is **little-endian** 4-byte words. Verified: e.g.
`mov r0,#1` → `E3A00001`, `sub r4,r5,r6,lsl #2` → `E0454106`, `bl .` →
`EBFFFFFE`. (The databook's bit-field *figures* were lost in Docling extraction,
so exact encodings come from the arbiter — the standard way we work.)

## Engine widening (Tier 2) — contained

The engine is 16-bit-address today, but the location counter (`pc`) and symbols
are already `i64`, so the widening is mostly public types and range checks:

- `Assembly.origin: u16 → u32` (+ `start`). Small ripple; the 16-bit containers
  (`sna` / `prg`) cast down.
- The `0..=0xFFFF` / `0x1_0000` range checks → a **dialect-configurable max
  address** (default `0xFFFF`, ARM `0x3FF_FFFF`), exactly like the `addr_unit`
  added for the CP1610.
- The ARM disassembler is **u32-native** (`disassemble_arm(code, origin: u32)`),
  so it doesn't disturb the ~40 existing `u16`-origin disassembler signatures.
- **Already present:** the 4-byte emit path (both endiannesses), and
  `Piece::Packed` — which handles the `B`/`BL` branch directly: `expr =
  target - (pc + 8)` (the ARM pipeline offset), `scale 4`, `mask 0xFFFFFF`,
  `or_bits = cond << 28 | 0xA << 24`.

## Proposed increments (sweep-verified, like the Z8000)

Large but regular ISA; the 15 condition codes are a uniform bits-31:28 prefix
handled from increment 1.

1. **Scaffold + data-processing register forms** — the 16 ALU opcodes
   (`AND`/`EOR`/`SUB`/`RSB`/`ADD`/`ADC`/`SBC`/`RSC`/`TST`/`TEQ`/`CMP`/`CMN`/
   `ORR`/`MOV`/`BIC`/`MVN`), `S` bit, condition infrastructure, the sweep harness,
   plus the u32 engine widening.
2. **Barrel shifter** — Op2 shifts (`LSL`/`LSR`/`ASR`/`ROR`/`RRX`), immediate and
   register shift amounts.
3. **Data-processing immediate** — the rotated-8-bit immediate encoding (find a
   valid rotation or error, matching vasm).
4. **Branch** — `B`/`BL` via `Piece::Packed` (pc+8, word-scaled).
5. **Single data transfer** — `LDR`/`STR` (offset / pre / post-index, writeback,
   byte/word).
6. **Block data transfer** — `LDM`/`STM` (register list, IA/IB/DA/DB, writeback,
   `^`).
7. **Multiply** — `MUL`/`MLA`.
8. **`SWI` + coprocessor** — `CDP`/`MRC`/`MCR`/`LDC`/`STC` (the FPA door; may be
   deferred).

## Reference

`reference/by-topic/cpu-arm/cpu-arm-reference.md` (distilled) + the VTI ARM
Databook 1990 (`vti-arm-databook-1990.md`). `vasmarm_std -m2` is the byte
arbiter. This is the roadmap's **Wave D** opener — see the umbrella
[`asm198x-cpu-coverage-roadmap.md`](../../decisions/asm198x-cpu-coverage-roadmap.md).
