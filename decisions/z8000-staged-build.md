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
6. **Shifts / rotates** (+ `EXTS`) — ✅ **landed (2026-07-03).** `SLA`/`SRA`/
   `SLL`/`SRL` (+ byte + long), `RL`/`RR`/`RLC`/`RRC` (+ byte), and
   `EXTSB`/`EXTS`/`EXTSL`. A `Shift` table (shift + rotate, a `ShiftKind`) keyed
   on `base6` 0x32 (byte) / 0x33 (word/long) with the register in the second
   byte's **high** nibble and the low nibble's bit 0 selecting shift (1) from
   rotate (0); a separate tiny `Extend` table (`base6` 0x31, top byte 0xB1). The
   key subtleties the probe pinned down: **`SLA`/`SRA` share one opcode** — the
   sign of the trailing count word picks the direction, so the dialect emits
   `+n` for left and `−n` for right and the disassembler reads the sign; the
   count word is a full **16-bit** signed value for word / long shifts but a
   signed **8-bit** value in the **low byte** (high byte zero) for byte shifts;
   the long shift/`EXTS` use of even `rr` pairs and `EXTSL`'s multiple-of-four
   **quad** `rq` register (a new `Size::Quad`) are enforced, odd/misaligned
   registers decoding as data to match `asl`. Rotates and `EXTS` (no count word)
   are opcode-sweep-verified; the shifts (whose count filler is out of range in
   the sweep, so they fall to data there) have a targeted round-trip. The long
   subops came out **SLLL/SRLL 5, SLAL/SRAL 0xD** and `EXTS` subops **EXTSB 0,
   EXTSL 7, EXTS 0xA** (probed, confirming the decision-record guesses).
   Byte-identical to `asl` (`cpu Z8002`).
7. **Bit** — ✅ **landed (2026-07-03).** `BIT`/`SET`/`RES` (+ byte), static and
   dynamic. A `Bit` table (`base6` `BIT` 0x27 / `SET` 0x25 / `RES` 0x23, byte
   forms one lower). The **static** form is dyadic-shaped: `MM base6 |
   field << 4 | b`, the operand reached by R / IR / DA / X exactly as the dyadic
   family, the **low nibble a bit number** (0–15 word, 0–7 byte), one word (+ an
   address word for DA / X). The **dynamic** form (bit number in a *word*
   register) is a two-word encoding at `MM` = 00 with the second byte's high
   nibble **zero** — which never collides with static `@Rn`, because **R0 is not
   a legal base register**, so the pointer field is always 1–15 and the zero slot
   is free. Word 1 is `base6 << 8 | bit-register`; word 2 is
   `target-register << 8` (register-only target, word or byte per the size). The
   static forms are opcode-sweep-verified; the dynamic form (its second word an
   out-of-range filler in the sweep, so it falls to data there) has a targeted
   round-trip. Byte-identical to `asl` (`cpu Z8002`).
8. **Multiply / divide** — ✅ **landed (2026-07-03).** `MULT`/`MULTL`/`DIV`/
   `DIVL`. A `MulDiv` table (`base6` `MULT` 0x19 / `MULTL` 0x18 / `DIV` 0x1B /
   `DIVL` 0x1A). These turned out **dyadic-shaped** — `MM base6 | field << 4 |
   dest`, the source reached by R / IM / IR / DA / X exactly as the dyadic family
   — but with **asymmetric operand sizes**: the destination is a double-width
   accumulator (a long `rr` pair for `MULT`/`DIV`, a quad `rq` for `MULTL`/
   `DIVL`) while the source (and its immediate width) is one size smaller (word
   for `MULT`/`DIV`, long for `MULTL`/`DIVL`). So the table carries **two** sizes
   (`dest` + `src`) rather than the dyadic single `size`; a shared `reg_aligned`
   helper enforces the even-`rr` / multiple-of-four-`rq` rules on both the
   accumulator and a register source (odd registers decode as data to match
   `asl`). The word-immediate forms (`MULT`/`DIV` `#imm`) are opcode-sweep-
   verified; the long-immediate forms (`MULTL`/`DIVL` `#imm`, a 4-byte immediate
   the sweep's 4-byte candidate can't hold) fall to data there, so a targeted
   round-trip guards them. Byte-identical to `asl` (`cpu Z8002`).
9. **Block / string** — ✅ **landed (2026-07-03).** The full repeat group (32
   instructions): block move `LDI`/`LDIR`/`LDD`/`LDDR` (+ byte), block compare
   `CPI`/`CPIR`/`CPD`/`CPDR` (+ byte), compare-string `CPSI`/…/`CPSDR` (+ byte),
   translate `TRIB`/`TRIRB`/`TRDB`/`TRDRB`, and translate-and-test
   `TRTIB`/…/`TRTDRB`. A `Block` table. All are **two-word** forms at `MM` = 10:
   word 1 is `TOP | pointer << 4 | op_nib`, word 2 is `count << 8 |
   pointer-or-register << 4 | ctrl` (word 2's top nibble always zero). Top bytes:
   `0xBB` word / `0xBA` byte for `LD`/`CP`/`CPS`, `0xB8` for the byte-only
   translate ops. Four operand shapes (a `BlockShape`): `LDx`/`CPSx` put the
   **source** pointer in word 1 and the **dest** pointer in word 2; `CPx` puts a
   **data register** (word/byte) in word 2 with a **condition code** in the
   control nibble; `TRxB`/`TRTxB` **reverse** it (dest in word 1, source in word
   2) and imply R1 (`RH1`). The control nibble is a fixed single/repeat marker
   for `LD`/`TR`/`TRT` but the condition code for `CP`/`CPS` (default 8 =
   *always*, omitted), reusing the increment-3 `cc_value`/`cc_name` table.
   `op_nib` identifies the op within a base6; `LDI`/`LDIR` (etc.) share it and
   split on the control nibble. **Not opcode-sweep-verified** — the sweep's
   4-byte candidate always has a nonzero word-2 top nibble, which no canonical
   block op has, so they all fall to data there; instead a **direct differential
   over all 32** (byte-identical to `asl`) plus a comprehensive round-trip guard
   the group. `cpu Z8002`. (`RLDB`/`RRDB` rotate-digit are *not* repeat
   instructions and are deferred to a later increment.)
10. **I/O** — ✅ **landed (2026-07-03).** The full privileged I/O group (44
    instructions): simple `IN`/`OUT`/`SIN`/`SOUT` (+ byte), block input
    `INI`/`INIR`/`IND`/`INDR` (+ byte), block output `OUTI`/`OTIR`/`OUTD`/`OTDR`
    (+ byte), and the special-I/O block versions `SINI`/… and `SOUTI`/… (+ byte).
    Two tables — a `SimpleIo` and a `BlockIo`. Everything is `MM` = 00. Simple
    I/O has a **direct**-port form (top `0x3B` word / `0x3A` byte, word 1 =
    `reg << 4 | sub`, `sub` = `IN` 4 / `SIN` 5 / `OUT` 6 / `SOUT` 7, then a port
    address word) and — for `IN`/`OUT` only — an **indirect** `@Rn`-port form
    (its own top byte `0x3D`/`0x3C`/`0x3F`/`0x3E`, word 1 = `port << 4 | reg`).
    Block I/O reuses the block/string two-word **Load** shape (`@Rd, @Rs, Rc`) at
    top `0x3B`/`0x3A`; the second byte's low nibble separates them (4–7 direct
    simple I/O, 0–3/8–B block I/O). The key operational discovery: **`asl`
    silently drops privileged instructions unless `SUPMODE ON` is set**, so
    `listing_z8000` now emits `supmode on` (harmless for every other
    instruction, and the dialect ignores the directive). Simple I/O is
    opcode-sweep-verified (with the `supmode on` header); the block-I/O forms
    fall to data there (word-2 zero top nibble) so a direct differential over all
    44 plus a round-trip guard the group. R0 is rejected as an indirect port (it
    is not a legal base). `cpu Z8002`.
11. **CPU control** — `NOP`/`HALT`/`EI`/`DI`/`LDCTL`/`LDPS`/`MSET`/flag ops.
12. **Segmented Z8001** — widen DA/X/RA address operands to segmented addresses
    as a target-extension over the non-segmented base (the 65816-over-6502
    pattern). Not new instructions — touches every memory-addressing op.

Each step: probe `asl` for the group's exact encodings, add the table rows +
dialect arm + decoder arm, extend the round-trip test, keep the sweep green,
commit.

## Increment 6 — shifts / rotates, encoding pre-decoded (2026-07-03)

Probed against `asl` (`cpu Z8002`) so the next session can skip re-probing.
The **shift** ops share a first word `MM base6 | reg << 4 | subop`, then a
**16-bit signed count word**: positive = left, negative = right (so `SLA`/`SRA`
are the *same* opcode, distinguished by the count's sign — the dialect emits
`+n` for `SLA`, `−n` for `SRA`; likewise `SLL`/`SRL`). Word `base6` = `0x33`,
byte = `0x32`, long = `0x33` with a distinct subop. `reg` is the high nibble.
Verified encodings:

- `sla r1,#4` → `B319 0004`; `sra r1,#4` → `B319 FFFC` (subop `9`, arithmetic).
- `sll r1,#4` → `B311 0004`; `srl r1,#4` → `B311 FFFC` (subop `1`, logical).
- `slab rl1,#3` → `B299 0003` (byte, `base6 0x32`, subop `9`).
- `sllb rl1,#3` → `B291 0003` (byte, subop `1`).
- `slal rr2,#8` → `B32D 0008` (long, subop `0xD`; `SLLL`/`SRLL` likely subop `5`
  — confirm by probe).

The **rotate** ops (`RL`/`RR`/`RLC`/`RRC`) take **no count word**; the count
(1 or 2) and the rotate type pack into the low nibble as `type·4 + (count−1)·2`,
with `RL 0`, `RR 1`, `RLC 2`, `RRC 3`. `base6 0x33` word / `0x32` byte, `reg` the
high nibble. Verified: `rl r1,#1` → `B310`, `rr r1,#2` → `B316`,
`rlc r1,#1` → `B318`, `rrc r1,#2` → `B31E`.

Note the base6 `0x32`/`0x33` sits in the same top-byte family as other ops, so
the shift/rotate decode keys on `base6` + subop; watch the low-nibble overlap
with the byte-vs-word `base6` (`0x32` byte, `0x33` word) and the register-field
placement (high nibble, unlike the dyadic low nibble). `EXTS`/`EXTSB`/`EXTSL`
(sign-extend) were seen at top byte `0xB1` — a separate small format to probe.
