# Z8000 — a staged, field-based build

**Status:** In progress (Wave B). Branch `feat/z8000`. Foundation landed
(sourced manual + decoded encoding model); implementation is staged in verified
increments.

## Why staged, when the other CPUs were one-shot

Every CPU landed so far (8080 … TMS9900) was a single-session, byte-identical
deliverable of roughly one spec + one dialect + one disassembler (~1–1.5k lines).
The **Z8000 is 3–4× that**: the Zilog manual counts **110 distinct instruction
types**, each with a subset of **eight addressing modes** (R, IM, IR, DA, X, RA,
BA, BX), **word / byte / long** sizes, and a **segmented (Z8001) vs non-segmented
(Z8002)** split that widens address operands. Attempting the whole ISA in one
pass invites exactly the bugs and "almost-there" churn the coding-cadence rules
warn against. So it is built as **verified increments**, each committed only when
the opcode-space sweep is green.

The sweep makes incremental delivery safe: `spec_sweep_matches_reference`
disassembles the opcode space, reassembles with `asl`, and compares. Any
instruction group **not yet implemented decodes to `word` data and is skipped**,
so a partial decoder is always self-consistent — each increment adds a group and
the sweep proves it byte-identical without disturbing the rest.

## Architecture

Like [PDP-11](../../decisions/asm198x-cpu-coverage-roadmap.md) and TMS9900, the
Z8000 is **field-packed** — operands live in fields inside the opcode word — so
`isa::z8000` is a **bespoke table** (mnemonic + base + a `Class` fixing the
field layout), keyed by both the dialect and a field-based disassembler, riding
the **computed-operand seam**. Big-endian; `asl`'s `h`-suffix hex (the 8080
lexer). Target `cpu Z8002` first.

## The decoded core: the dyadic family

The arithmetic / logic / load family (`ADD`/`SUB`/`OR`/`AND`/`XOR`/`CP`/`LD` +
`ADC`/`SBC` + byte forms) shares one first-word format:

```
  MM ooooo b   ssss dddd
```

- `MM` (bits 15–14) = addressing-mode group: `00` → **IR** (`@Rs`) if `ssss ≠ 0`
  else **IM** (immediate word follows); `01` → **DA** (address word) if
  `ssss = 0` else **X** (`addr(Rs)`, address word); `10` → **R** (`Rs`).
- `ooooo` (bits 13–9) = operation: `ADD 0x00`, `SUB 0x01`, `OR 0x02`, `AND 0x03`,
  `XOR 0x04`, `CP 0x05`, `LD 0x10`, `ADC 0x15`, `SBC 0x17`.
- `b` (bit 8) = word (1) / byte (0). Byte registers are `RHn = n`, `RLn = 8+n`.
- second byte = `ssss dddd` (source field, destination register).

Verified against `asl`: `ld r1,r2 = A121`, `add r1,#5 = 0101 0005`,
`add r1,1234h(r2) = 4121 1234`, `addb rl1,rl2 = 80A9`.

## Increment plan

1. **Dyadic word/byte core** — ✅ **landed (2026-07-02).** The family above
   across R / IM / IR / DA / X, plus the `LD`/`LDB` store forms. Established the
   addressing-mode parser + the field decoder + the opcode-space sweep.
   Byte-identical to `asl` (`cpu Z8002`). (Correction from first probe: `ADC` is
   operation `0x1A`, `SBC` `0x1B`; byte immediates replicate the byte into both
   halves of the word.)
2. **Long dyadic + `EX` + `LDA`** — ✅ **landed (2026-07-02).** `LDL`/`ADDL`/
   `SUBL`/`CPL` (+ `LDL` store), `EX`/`EXB`, and `LDA`. This is also where the
   increment-1 table was generalised to a **`base6` + `Size` + modes-bitmask**
   model: a form's top byte is `MM << 6 | base6`, the `Size` (byte/word/long/
   address) fixes register naming and immediate width, and the modes bitmask
   gates which addressing modes each entry allows (so `EX` rejects immediate,
   `LDA` is direct/indexed only, `ADC`/`SBC` are register only). `LDR` moved to
   the program-control increment (it is PC-relative) and `CLR` to the
   single-operand increment (a different, low-nibble-keyed format).
3. **Program control** — ✅ **landed (2026-07-03).** `JP cc,dst`, `CALL`,
   `JR cc`, `DJNZ`/`DBJNZ`, `CALR`, `RET cc`. Added a separate `Ctl` table +
   `CtlKind` (the control formats diverge from the dyadic field layout) and the
   shared condition-code table (`cc_value`/`cc_name`; code 8 = always = no
   mnemonic). The relative ops reuse the PDP-11 `Piece::Packed` word-scale:
   `JR` is `(target − PC)/2` signed 8-bit, `DJNZ` `(PC − target)/2` 7-bit
   backward, `CALR` `(PC − target)/2` signed 12-bit. `JP`/`CALL`/`RET` are
   sweep-verified; the relative `JR`/`DJNZ`/`CALR` (position-dependent, dropped
   by the sweep) have a targeted round-trip. `LDR` (PC-relative load) is deferred
   to the relative-data / single-operand increment.
4. **Single-operand ALU** — ✅ **landed (2026-07-03).** `CLR`/`COM`/`NEG`/
   `TEST`/`TSET` (+ byte) and `INC`/`DEC` (+ byte, count 1–16). A separate `Mono`
   table: the operand register/pointer/index is the second byte's **high**
   nibble, the low nibble a fixed sub-opcode (`COM 0`, `NEG 2`, `TEST 4`,
   `TSET 6`, `CLR 8`) or `count − 1` (`INC`/`DEC`). R / IR / DA / X modes (no
   immediate). `PUSH`/`POP`/`PUSHL`/`POPL` and `EXTS` move to increment 5 (they
   have their own two-operand / sign-extend formats).
5. **Stack** — ✅ **landed (2026-07-03).** `PUSH`/`POP`/`PUSHL`/`POPL`. A
   separate `Stack` table: the stack-pointer register is the second byte's high
   nibble and the value operand's field the low nibble, `MM` selecting the
   value's mode (R / IR / DA / X). Syntax is `PUSH @Rsp, src` / `POP dst, @Rsp`
   (pointer leads a push, trails a pop). `PUSH #imm` is a special opcode
   (`base6` 0x0D, low nibble 9). `PUSHL`/`POPL` are long; only `PUSH` has an
   immediate form.
6. **Shifts / rotates** — `SLA`/`SRA`/`SLL`/`SRL` (a signed count word, `+`
   left / `−` right) and `RL`/`RR`/`RLC`/`RRC` (count 1–2 packed in the low
   nibble). `EXTS` folds in here too.
6. **Bit** — `BIT`/`SET`/`RES`, static and dynamic.
7. **Multiply / divide, block, string, I/O, CPU control** — the remainder.
8. **Segmented Z8001** — widen DA/X/RA address operands to segmented addresses
   as a target-extension over the non-segmented base.

Each step: probe `asl` for the group's exact encodings, add the `Class` +
table rows + dialect arm + decoder arm, extend the round-trip test, keep the
sweep green, commit.
